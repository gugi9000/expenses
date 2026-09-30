use leptos::prelude::*;
use leptos_meta::{MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::{
    ParamSegment, StaticSegment,
    components::{A, Outlet, ParentRoute, Redirect, Route, Router, Routes},
    hooks::use_query_map,
};

use crate::{
    i18n::t,
    model::SessionUser,
    pages::{ExpenseDetailPage, ExpenseListPage},
};

#[server]
pub async fn get_session_user() -> Result<Option<SessionUser>, ServerFnError> {
    crate::server::auth::session_user().await
}

#[server]
pub async fn entra_enabled() -> Result<bool, ServerFnError> {
    Ok(expect_context::<crate::server::state::AppState>().entra.is_some())
}

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="da">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
                <meta name="theme-color" content="#1f4e79" />
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
                </ParentRoute>
            </Routes>
        </Router>
    }
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
                        view! {
                            <Header user />
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
fn Header(user: SessionUser) -> impl IntoView {
    let is_admin = user.is_admin();
    view! {
        <header class="topbar">
            <A href="/" attr:class="brand">{t::APP_NAME}</A>
            <nav>
                <A href="/">{t::NAV_EXPENSES}</A>
                {is_admin.then(|| view! { <span class="badge">{t::ROLE_ADMIN}</span> })}
            </nav>
            <form method="post" action="/auth/logout">
                <button class="link" type="submit" title=user.display_name.clone()>{t::LOGOUT}</button>
            </form>
        </header>
    }
}
