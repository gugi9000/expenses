pub mod admin_pages;
pub mod api;
pub mod app;
pub mod i18n;
pub mod model;
pub mod pages;
#[cfg(feature = "ssr")]
pub mod server;
pub mod sheet_pages;
pub mod upload;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}
