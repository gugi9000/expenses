use std::path::PathBuf;

use anyhow::{Context, Result, bail};

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub data_dir: PathBuf,
    /// Public origin, e.g. `https://udgifter.example.dk`; used for redirects and the Origin check.
    pub base_url: String,
    pub cookie_secure: bool,
    /// Only trust `X-Forwarded-For` when running behind a known reverse proxy.
    pub trust_proxy: bool,
    pub session_hours: i64,
    pub entra: Option<EntraConfig>,
}

#[derive(Clone, Debug)]
pub struct EntraConfig {
    pub tenant_id: String,
    pub client_id: String,
    pub client_secret: String,
    pub admin_role: String,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn flag(name: &str, default: bool) -> Result<bool> {
    match var(name).as_deref() {
        None => Ok(default),
        Some("1" | "true" | "yes") => Ok(true),
        Some("0" | "false" | "no") => Ok(false),
        Some(other) => bail!("{name} must be true/false, got {other:?}"),
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();

        let data_dir = PathBuf::from(var("DATA_DIR").unwrap_or_else(|| "data".into()));
        let database_url = var("DATABASE_URL").unwrap_or_else(|| {
            format!(
                "sqlite://{}?mode=rwc",
                data_dir.join("expenses.db").display()
            )
        });
        let base_url = var("BASE_URL")
            .unwrap_or_else(|| "http://localhost:3000".into())
            .trim_end_matches('/')
            .to_string();

        let tenant_id = var("ENTRA_TENANT_ID").or_else(|| var("ENTRA_TENANT"));
        let entra = match (
            tenant_id,
            var("ENTRA_CLIENT_ID"),
            var("ENTRA_CLIENT_SECRET"),
        ) {
            (Some(tenant_id), Some(client_id), Some(client_secret)) => Some(EntraConfig {
                tenant_id,
                client_id,
                client_secret,
                admin_role: var("ENTRA_ADMIN_ROLE").unwrap_or_else(|| "Expenses.Admin".into()),
            }),
            (None, None, None) => None,
            _ => bail!(
                "ENTRA_TENANT_ID, ENTRA_CLIENT_ID and ENTRA_CLIENT_SECRET must all be set, or none of them"
            ),
        };

        let session_hours = var("SESSION_HOURS")
            .map(|v| v.parse::<i64>())
            .transpose()
            .context("SESSION_HOURS must be a number")?
            .unwrap_or(12);

        Ok(Self {
            database_url,
            data_dir,
            cookie_secure: flag("COOKIE_SECURE", base_url.starts_with("https://"))?,
            trust_proxy: flag("TRUST_PROXY", false)?,
            base_url,
            session_hours,
            entra,
        })
    }
}
