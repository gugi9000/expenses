use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet, EndpointSet,
    IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
    reqwest,
};
use serde::Deserialize;
use tokio::sync::RwLock;

use super::config::EntraConfig;

type EntraClient =
    CoreClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointMaybeSet, EndpointMaybeSet>;

/// Re-discover periodically so rotated signing keys are picked up.
const METADATA_TTL: Duration = Duration::from_secs(6 * 60 * 60);

pub struct Entra {
    config: EntraConfig,
    redirect_url: RedirectUrl,
    http: reqwest::Client,
    metadata: RwLock<Option<(Instant, CoreProviderMetadata)>>,
}

pub struct AuthStart {
    pub url: String,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

pub struct EntraIdentity {
    pub oid: String,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub is_admin: bool,
}

/// Microsoft-specific claims not modelled by `openidconnect`.
#[derive(Deserialize)]
struct MicrosoftClaims {
    oid: Option<String>,
    tid: Option<String>,
    #[serde(default)]
    roles: Vec<String>,
}

impl Entra {
    pub fn new(config: EntraConfig, base_url: &str) -> Result<Self> {
        let http = reqwest::ClientBuilder::new()
            // Following redirects here would enable SSRF via the token endpoint.
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()?;
        Ok(Self {
            redirect_url: RedirectUrl::new(format!("{base_url}/auth/entra/callback"))?,
            config,
            http,
            metadata: RwLock::new(None),
        })
    }

    async fn client(&self) -> Result<EntraClient> {
        let cached = self
            .metadata
            .read()
            .await
            .as_ref()
            .filter(|(fetched, _)| fetched.elapsed() < METADATA_TTL)
            .map(|(_, m)| m.clone());

        let metadata = match cached {
            Some(m) => m,
            None => {
                let issuer = IssuerUrl::new(format!(
                    "https://login.microsoftonline.com/{}/v2.0",
                    self.config.tenant_id
                ))?;
                let m = CoreProviderMetadata::discover_async(issuer, &self.http)
                    .await
                    .context("Entra OIDC discovery failed")?;
                *self.metadata.write().await = Some((Instant::now(), m.clone()));
                m
            }
        };

        Ok(CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(self.config.client_id.clone()),
            Some(ClientSecret::new(self.config.client_secret.clone())),
        )
        .set_redirect_uri(self.redirect_url.clone()))
    }

    pub async fn start(&self) -> Result<AuthStart> {
        let client = self.client().await?;
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, state, nonce) = client
            .authorize_url(CoreAuthenticationFlow::AuthorizationCode, CsrfToken::new_random, Nonce::new_random)
            .add_scope(Scope::new("profile".into()))
            .add_scope(Scope::new("email".into()))
            .set_pkce_challenge(pkce_challenge)
            .url();
        Ok(AuthStart {
            url: url.to_string(),
            state: state.secret().clone(),
            nonce: nonce.secret().clone(),
            pkce_verifier: pkce_verifier.secret().clone(),
        })
    }

    pub async fn finish(&self, code: String, nonce: String, pkce_verifier: String) -> Result<EntraIdentity> {
        let client = self.client().await?;
        let token = client
            .exchange_code(AuthorizationCode::new(code))?
            .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
            .request_async(&self.http)
            .await
            .context("token exchange failed")?;

        let id_token = token.id_token().ok_or_else(|| anyhow!("no id_token in response"))?;
        let claims = id_token
            .claims(&client.id_token_verifier(), &Nonce::new(nonce))
            .context("id_token verification failed")?;

        // Signature, issuer, audience and nonce are verified above; this only reads extra claims.
        let raw = id_token.to_string();
        let payload = raw.split('.').nth(1).ok_or_else(|| anyhow!("malformed id_token"))?;
        let ms: MicrosoftClaims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload)?)?;

        if ms.tid.as_deref() != Some(self.config.tenant_id.as_str()) {
            bail!("id_token from unexpected tenant");
        }
        let oid = ms.oid.ok_or_else(|| anyhow!("id_token missing oid"))?;

        let email = claims.email().map(|e| e.as_str().to_string());
        let username = claims
            .preferred_username()
            .map(|u| u.as_str().to_string())
            .or_else(|| email.clone())
            .unwrap_or_else(|| oid.clone());
        let display_name = claims
            .name()
            .and_then(|n| n.get(None))
            .map(|n| n.as_str().to_string())
            .unwrap_or_else(|| username.clone());

        Ok(EntraIdentity {
            is_admin: ms.roles.iter().any(|r| r == &self.config.admin_role),
            oid,
            username,
            display_name,
            email,
        })
    }
}
