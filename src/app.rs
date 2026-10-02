use std::time::Duration;

use leptos::{prelude::*, task::spawn_local};
use leptos_meta::{Html, MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::{
    ParamSegment, StaticSegment,
    components::{A, Outlet, ParentRoute, Redirect, Route, Router, Routes},
    hooks::use_query_map,
};

use crate::{
    admin_pages::{
        AdminExpensePage, AdminExpensesPage, AdminLayout, AdminLogPage, AdminSheetPage,
        AdminSheetsPage, AdminUsersPage,
    },
    i18n::t,
    model::{SessionUser, Theme},
    pages::{ExpenseDetailPage, ExpenseListPage},
    sheet_pages::{NewSheetPage, SheetDetailPage, SheetListPage},
};

/// Identifies the exact build; differs between server and client after a deploy.
pub const BUILD_ID: &str = env!("BUILD_ID");

#[server]
pub async fn get_session_user() -> Result<Option<SessionUser>, ServerFnError> {
    crate::server::auth::session_user().await
}

#[server]
pub async fn entra_enabled() -> Result<bool, ServerFnError> {
    Ok(use_context::<crate::server::state::AppState>().is_some_and(|s| s.entra.is_some()))
}

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="da">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
                <meta name="theme-color" content="#1f4e79" />
                <link rel="icon" href="/favicon.svg" type="image/svg+xml" />
                <link rel="apple-touch-icon" href="/apple-touch-icon.png" />
                <link rel="manifest" href="/manifest.webmanifest" />
                <meta name="apple-mobile-web-app-title" content=t::APP_NAME />
                <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent" />
                <AutoReload options=options.clone() />
                <HydrationScripts options />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/expenses.css" />
        <Title text=t::APP_NAME />
        <Router>
            <Routes fallback=|| view! { <p class="page-message">{t::NOT_FOUND}</p> }>
                <Route path=StaticSegment("login") view=LoginPage />
                <ParentRoute path=StaticSegment("") view=AuthedLayout>
                    <Route path=StaticSegment("") view=ExpenseListPage />
                    <Route path=(StaticSegment("bilag"), ParamSegment("id")) view=ExpenseDetailPage />
                    <Route path=StaticSegment("afregninger") view=SheetListPage />
                    <Route path=(StaticSegment("afregninger"), StaticSegment("ny")) view=NewSheetPage />
                    <Route path=(StaticSegment("afregninger"), ParamSegment("id")) view=SheetDetailPage />
                    <ParentRoute path=StaticSegment("admin") view=AdminLayout>
                        <Route path=StaticSegment("") view=AdminExpensesPage />
                        <Route path=(StaticSegment("bilag"), ParamSegment("id")) view=AdminExpensePage />
                        <Route path=StaticSegment("afregninger") view=AdminSheetsPage />
                        <Route path=(StaticSegment("afregninger"), ParamSegment("id")) view=AdminSheetPage />
                        <Route path=StaticSegment("brugere") view=AdminUsersPage />
                        <Route path=StaticSegment("log") view=AdminLogPage />
                    </ParentRoute>
                </ParentRoute>
            </Routes>
        </Router>
        <UpdateBanner />
        <footer class="app-footer">{t::FOOTER_BRAND}" · v"{env!("CARGO_PKG_VERSION")}</footer>
    }
}

#[component]
fn UpdateBanner() -> impl IntoView {
    let outdated = RwSignal::new(false);
    // Effects only run in the browser.
    Effect::new(move |_| watch_build_id(outdated));

    view! {
        <Show when=move || outdated.get()>
            <div class="update-banner" role="status">
                <span>{t::UPDATE_AVAILABLE}</span>
                <button class="button" type="button" on:click=|_| { let _ = window().location().reload(); }>
                    {t::UPDATE_RELOAD}
                </button>
            </div>
        </Show>
    }
}

fn watch_build_id(outdated: RwSignal<bool>) {
    use wasm_bindgen::{JsCast, closure::Closure};

    let check = move || {
        spawn_local(async move {
            if fetch_build_id().await.is_some_and(|id| id != BUILD_ID) {
                outdated.set(true);
            }
        });
    };
    check();
    let on_visible = Closure::<dyn Fn()>::new(move || {
        if document().visibility_state() == web_sys::VisibilityState::Visible {
            check();
        }
    });
    let _ = document()
        .add_event_listener_with_callback("visibilitychange", on_visible.as_ref().unchecked_ref());
    on_visible.forget();
    set_interval(check, Duration::from_secs(300));
}

async fn fetch_build_id() -> Option<String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let init = web_sys::RequestInit::new();
    init.set_cache(web_sys::RequestCache::NoStore);
    let res: web_sys::Response =
        JsFuture::from(window().fetch_with_str_and_init("/version", &init))
            .await
            .ok()?
            .dyn_into()
            .ok()?;
    if !res.ok() {
        return None;
    }
    let text = JsFuture::from(res.text().ok()?).await.ok()?.as_string()?;
    let id = text.trim();
    (!id.is_empty()).then(|| id.to_owned())
}

fn login_error_text(code: &str) -> &'static str {
    match code {
        "spaerret" => t::LOGIN_RATE_LIMITED,
        "microsoft" => t::LOGIN_ENTRA_FAILED,
        "deaktiveret" => t::LOGIN_DISABLED,
        _ => t::LOGIN_FAILED,
    }
}

#[component]
fn LoginPage() -> impl IntoView {
    let query = use_query_map();
    let error = move || query.read().get("fejl").map(|c| login_error_text(&c));
    let user = Resource::new_blocking(|| (), |_| get_session_user());
    let entra = Resource::new_blocking(|| (), |_| entra_enabled());

    view! {
        <Title text=t::LOGIN_TITLE />
        <main class="login">
            <h1>{t::APP_NAME}</h1>
            <Suspense fallback=|| view! { <p>{t::LOADING}</p> }>
                {move || Suspend::new(async move {
                    if matches!(user.await, Ok(Some(_))) {
                        return view! { <Redirect path="/" /> }.into_any();
                    }
                    let entra = entra.await.unwrap_or(false);
                    view! {
                        {move || error().map(|e| view! { <p class="alert" role="alert">{e}</p> })}
                        {entra.then(|| view! {
                            <a class="button primary block" href="/auth/entra/login" rel="external">
                                {t::LOGIN_WITH_MICROSOFT}
                            </a>
                            <p class="divider"><span>{t::OR}</span></p>
                        })}
                        <form class="card" method="post" action="/auth/login">
                            <h2>{t::LOGIN_LOCAL_HEADING}</h2>
                            <label>
                                {t::USERNAME}
                                <input name="username" autocomplete="username" autocapitalize="none" required />
                            </label>
                            <label>
                                {t::PASSWORD}
                                <input name="password" type="password" autocomplete="current-password" required />
                            </label>
                            <button class="button block" type="submit">{t::LOGIN_SUBMIT}</button>
                        </form>
                    }
                        .into_any()
                })}
            </Suspense>
        </main>
    }
}

#[component]
fn AuthedLayout() -> impl IntoView {
    let user = Resource::new_blocking(|| (), |_| get_session_user());

    view! {
        <Suspense fallback=|| view! { <p class="page-message">{t::LOADING}</p> }>
            {move || Suspend::new(async move {
                match user.await {
                    Ok(Some(user)) => {
                        provide_context(user.clone());
                        let theme = RwSignal::new(user.theme);
                        view! {
                            <Html {..} data-theme=move || theme.get().as_str() />
                            <Header user theme />
                            <main class="content">
                                <Outlet />
                            </main>
                        }
                            .into_any()
                    }
                    Ok(None) => view! { <Redirect path="/login" /> }.into_any(),
                    Err(_) => view! { <p class="alert">{t::GENERIC_ERROR}</p> }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn Header(user: SessionUser, theme: RwSignal<Theme>) -> impl IntoView {
    let is_admin = user.is_admin();
    view! {
        <header class="topbar">
            <A href="/" attr:class="brand">{t::APP_NAME}</A>
            <nav>
                <A href="/">{t::NAV_EXPENSES}</A>
                <A href="/afregninger">{t::NAV_SHEETS}</A>
                {is_admin.then(|| view! { <A href="/admin">{t::NAV_ADMIN}</A> })}
            </nav>
        </header>
        <div class="userbar">
            <span class="who">
                <span class="visually-hidden">{t::LOGGED_IN_AS}" "</span>
                <strong>{user.display_name}</strong>
            </span>
            {is_admin.then(|| view! { <span class="badge">{t::ROLE_ADMIN}</span> })}
            <ThemePicker theme />
            <form method="post" action="/auth/logout">
                <button class="link" type="submit">{t::LOGOUT}</button>
            </form>
        </div>
    }
}

#[component]
fn ThemePicker(theme: RwSignal<Theme>) -> impl IntoView {
    let on_change = move |ev| {
        let Some(new) = Theme::parse(&event_target_value(&ev)) else {
            return;
        };
        theme.set(new);
        spawn_local(async move {
            let _ = crate::api::set_theme(new).await;
        });
    };
    let option = move |value: Theme, label: &'static str| {
        view! { <option value=value.as_str() selected=move || theme.get() == value>{label}</option> }
    };

    view! {
        <label class="theme-picker">
            <span class="visually-hidden">{t::THEME}</span>
            <select prop:value=move || theme.get().as_str() on:change=on_change>
                {option(Theme::System, t::THEME_SYSTEM)}
                {option(Theme::Light, t::THEME_LIGHT)}
                {option(Theme::Dark, t::THEME_DARK)}
            </select>
        </label>
    }
}
