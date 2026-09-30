#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use std::{net::SocketAddr, sync::Arc, time::Duration};

    use axum::{
        Router, body::Body, extract::State, http::Request, middleware, response::IntoResponse,
        routing::post,
    };
    use expenses::{
        app::{App, shell},
        server::{
            auth, config::Config, db, entra::Entra, expenses::routes as expense_routes,
            files::FileStore, fx, ocr::NoopOcr, security, session, state::AppState,
        },
    };
    use leptos::prelude::*;
    use leptos_axum::{LeptosRoutes, generate_route_list, handle_server_fns_with_context};
    use tracing_subscriber::EnvFilter;

    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    let config = Config::from_env()?;
    let pool = db::connect(&config).await?;
    let entra = config
        .entra
        .clone()
        .map(|e| Entra::new(e, &config.base_url).map(Arc::new))
        .transpose()?;
    if entra.is_none() {
        tracing::warn!("Entra ID not configured; only local users can log in");
    }

    let conf = get_configuration(None)?;
    let addr = conf.leptos_options.site_addr;
    let state = AppState {
        leptos_options: conf.leptos_options,
        pool: pool.clone(),
        files: Arc::new(FileStore::new(&config.data_dir)),
        ocr: Arc::new(NoopOcr),
        config: Arc::new(config),
        entra,
    };

    fx::spawn_updater(pool.clone())?;

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if let Err(e) = session::purge_expired(&pool).await {
                tracing::error!("purging expired sessions failed: {e}");
            }
        }
    });

    async fn server_fn_handler(
        State(state): State<AppState>,
        req: Request<Body>,
    ) -> impl IntoResponse {
        handle_server_fns_with_context(move || provide_context(state.clone()), req).await
    }

    let routes = generate_route_list(App);
    let app = Router::new()
        .route("/api/{*fn_name}", post(server_fn_handler))
        .merge(auth::routes())
        .merge(expense_routes())
        .leptos_routes_with_context(
            &state,
            routes,
            {
                let state = state.clone();
                move || provide_context(state.clone())
            },
            {
                let options = state.leptos_options.clone();
                move || shell(options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler::<AppState, _>(shell))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security::require_same_origin,
        ))
        .with_state(state);

    tracing::info!("listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}

#[cfg(not(feature = "ssr"))]
fn main() {}
