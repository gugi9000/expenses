use std::sync::Arc;

use axum::extract::FromRef;
use leptos::config::LeptosOptions;
use sqlx::SqlitePool;

use super::{config::Config, entra::Entra, files::FileStore, ocr::OcrProvider};

#[derive(Clone)]
pub struct AppState {
    pub leptos_options: LeptosOptions,
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    pub entra: Option<Arc<Entra>>,
    pub files: Arc<FileStore>,
    pub ocr: Arc<dyn OcrProvider>,
}

impl FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}
