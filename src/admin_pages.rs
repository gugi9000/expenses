//! Read-only admin views of all users' data. The server enforces the admin role on every call.

use leptos::prelude::*;
use leptos_router::{
    components::{A, Outlet},
    hooks::{use_navigate, use_params_map, use_query_map},
};

use crate::{
    api::{
        admin_audit_log, admin_get_expense, admin_get_sheet, admin_list_expenses,
        admin_list_sheets, admin_list_users, error_text, list_categories,
    },
    i18n::{audit_action_label, format_amount, format_date, format_datetime, format_rate, t},
    model::{
        AdminUser, AuditEntry, BASE_CURRENCY, ExpenseDetail, ExpenseStatus, Role, SessionUser,
        SheetSummary,
    },
    pages::{STATUS_FILTERS, attachment_view, expense_card_body, status_badge},
    sheet_pages::{SheetView, sheet_badge},
};

fn query_i64(key: &'static str) -> impl Fn() -> Option<i64> + Copy {
    let query = use_query_map();
    move || query.read().get(key).and_then(|v| v.parse().ok())
}

fn href(path: &str, params: &[(&str, Option<String>)]) -> String {
    let query: Vec<String> = params
        .iter()
        .filter_map(|(k, v)| v.as_ref().map(|v| format!("{k}={v}")))
        .collect();
    if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{}", query.join("&"))
    }
}

fn loading() -> impl IntoView {
    view! { <p class="page-message">{t::LOADING}</p> }
}

#[component]
pub fn AdminLayout() -> impl IntoView {
    let is_admin = use_context::<SessionUser>().is_some_and(|u| u.is_admin());
    if !is_admin {
        return view! { <p class="alert">{t::ERR_FORBIDDEN}</p> }.into_any();
    }
    view! {
        <h1>{t::ADMIN_TITLE}</h1>
        <nav class="chips tabs">
            <A href="/admin" exact=true attr:class="chip">{t::ADMIN_TAB_EXPENSES}</A>
            <A href="/admin/afregninger" attr:class="chip">{t::ADMIN_TAB_SHEETS}</A>
            <A href="/admin/brugere" attr:class="chip">{t::ADMIN_TAB_USERS}</A>
            <A href="/admin/log" attr:class="chip">{t::ADMIN_TAB_LOG}</A>
        </nav>
        <p class="notice small">{t::ADMIN_READ_ONLY}</p>
        <Outlet />
    }
    .into_any()
}

/// A user picker that navigates via `to_href(selected_user)`.
#[component]
fn UserSelect(selected: Option<i64>, to_href: fn(Option<i64>) -> String) -> impl IntoView {
    let users = Resource::new(|| (), |_| admin_list_users());
    let navigate = use_navigate();
    let on_change = move |ev: leptos::ev::Event| {
        navigate(
            &to_href(event_target_value(&ev).parse().ok()),
            Default::default(),
        );
    };
    view! {
        <Suspense>
            {move || {
                let on_change = on_change.clone();
                Suspend::new(async move {
                    let users = users.await.unwrap_or_default();
                    view! {
                        <select class="filter" aria-label=t::OWNER on:change=on_change>
                            <option value="" selected=selected.is_none()>{t::ALL_USERS}</option>
                            {users
                                .into_iter()
                                .map(|u| {
                                    view! {
                                        <option value=u.id.to_string() selected=selected == Some(u.id)>
                                            {u.display_name}
                                        </option>
                                    }
                                })
                                .collect_view()}
                        </select>
                    }
                })
            }}
        </Suspense>
    }
}

#[component]
pub fn AdminExpensesPage() -> impl IntoView {
    let query = use_query_map();
    let user = query_i64("bruger");
    let status = move || {
        query
            .read()
            .get("status")
            .filter(|s| ExpenseStatus::parse(s).is_some())
    };
    let expenses = Resource::new(
        move || (user(), status()),
        |(u, s)| admin_list_expenses(u, s),
    );

    view! {
        <UserSelect
            selected=user()
            to_href=|u| href("/admin", &[("bruger", u.map(|u| u.to_string()))])
        />
        <nav class="chips">
            {STATUS_FILTERS
                .into_iter()
                .map(|f| {
                    let code = f.map(|s| s.as_str());
                    view! {
                        <a
                            class="chip"
                            class:active=move || status().as_deref() == code
                            href=move || {
                                href(
                                    "/admin",
                                    &[
                                        ("bruger", user().map(|u| u.to_string())),
                                        ("status", code.map(str::to_string)),
                                    ],
                                )
                            }
                        >
                            {f.map_or(t::FILTER_ALL, |s| s.label())}
                        </a>
                    }
                })
                .collect_view()}
        </nav>
        <Transition fallback=loading>
            {move || Suspend::new(async move {
                match expenses.await {
                    Ok(items) if items.is_empty() => view! { <p class="empty">{t::NO_MATCHING}</p> }.into_any(),
                    Ok(items) => {
                        view! {
                            <ul class="expense-list">
                                {items
                                    .into_iter()
                                    .map(|item| {
                                        let link = format!("/admin/bilag/{}", item.id);
                                        let deleted = item.deleted;
                                        view! {
                                            <li>
                                                <a class="expense-card" class:deleted=deleted href=link>
                                                    {expense_card_body(item, true)}
                                                </a>
                                            </li>
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                        }
                            .into_any()
                    }
                    Err(e) => view! { <p class="alert">{error_text(&e)}</p> }.into_any(),
                }
            })}
        </Transition>
    }
}

#[component]
pub fn AdminExpensePage() -> impl IntoView {
    let params = use_params_map();
    let id = move || {
        params
            .read()
            .get("id")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_default()
    };
    let expense = Resource::new(id, admin_get_expense);
    let categories = Resource::new(|| (), |_| list_categories());

    view! {
        <a class="back" href="/admin">"← "{t::BACK}</a>
        <Transition fallback=loading>
            {move || Suspend::new(async move {
                let categories = categories.await.unwrap_or_default();
                match expense.await {
                    Ok(Some(e)) => {
                        let category = e
                            .category_id
                            .and_then(|id| categories.into_iter().find(|c| c.id == id))
                            .map(|c| c.name);
                        expense_facts(e, category).into_any()
                    }
                    Ok(None) => view! { <p class="empty">{t::VOUCHER_NOT_FOUND}</p> }.into_any(),
                    Err(e) => view! { <p class="alert">{error_text(&e)}</p> }.into_any(),
                }
            })}
        </Transition>
    }
}

fn expense_facts(e: ExpenseDetail, category: Option<String>) -> impl IntoView {
    let amount = e
        .amount_minor
        .zip(e.currency.clone())
        .map(|(a, c)| format_amount(a, &c));
    let converted = (e.currency.as_deref() != Some(BASE_CURRENCY))
        .then(|| {
            e.amount_base_minor
                .zip(e.fx_rate.clone())
                .zip(e.fx_rate_date)
        })
        .flatten()
        .map(|((base, rate), date)| {
            format!(
                "{} ({} {}: {})",
                format_amount(base, BASE_CURRENCY),
                t::FX_RATE_FROM,
                format_date(date),
                format_rate(&rate)
            )
        });
    let fact = |label: &'static str, value: Option<String>| {
        view! {
            <dt>{label}</dt>
            <dd>{value.unwrap_or_else(|| t::NOT_SET.to_string())}</dd>
        }
    };

    view! {
        <div class="detail-head">
            <h1>{e.vendor.clone().unwrap_or_else(|| e.kind.label().to_string())}</h1>
            {status_badge(e.status)}
        </div>
        <p>
            <strong>{format!("{}: {}", t::OWNER, e.owner_name)}</strong>
            {e.deleted.then(|| view! { " " <span class="status status-invalid">{t::DELETED}</span> })}
        </p>
        {(!e.duplicates_of.is_empty())
            .then(|| {
                view! {
                    <p class="alert">
                        {t::DUPLICATE_WARNING}" "
                        {e
                            .duplicates_of
                            .iter()
                            .map(|d| view! { <a href=format!("/admin/bilag/{d}")>{format!("#{d}")}</a>" " })
                            .collect_view()}
                    </p>
                }
            })}
        <div class="pages">{e.attachments.into_iter().map(attachment_view).collect_view()}</div>
        <dl class="card facts">
            {fact(t::FIELD_KIND, Some(e.kind.label().to_string()))}
            {fact(t::FIELD_CATEGORY, category)}
            {fact(t::FIELD_VENDOR, e.vendor)}
            {fact(t::FIELD_DATE, e.expense_date.map(format_date))}
            {fact(t::FIELD_AMOUNT, amount)}
            {converted.map(|c| fact(t::CONVERTED, Some(c)))}
            {fact(t::FIELD_DESCRIPTION, e.description)}
        </dl>
    }
}

#[component]
pub fn AdminSheetsPage() -> impl IntoView {
    let user = query_i64("bruger");
    let sheets = Resource::new(user, admin_list_sheets);
    view! {
        <UserSelect
            selected=user()
            to_href=|u| href("/admin/afregninger", &[("bruger", u.map(|u| u.to_string()))])
        />
        <Transition fallback=loading>
            {move || Suspend::new(async move {
                match sheets.await {
                    Ok(list) if list.is_empty() => view! { <p class="empty">{t::NO_SHEETS}</p> }.into_any(),
                    Ok(list) => {
                        view! { <ul class="expense-list">{list.into_iter().map(admin_sheet_card).collect_view()}</ul> }
                            .into_any()
                    }
                    Err(e) => view! { <p class="alert">{error_text(&e)}</p> }.into_any(),
                }
            })}
        </Transition>
    }
}

fn admin_sheet_card(s: SheetSummary) -> impl IntoView {
    view! {
        <li>
            <a class="sheet-card" href=format!("/admin/afregninger/{}", s.id)>
                <div class="line small owner">
                    <span>{s.owner_name}</span>
                </div>
                <div class="line">
                    <strong class="title">{s.title}</strong>
                    <span class="amount">{format_amount(s.total_base_minor, BASE_CURRENCY)}</span>
                </div>
                <div class="line muted small">
                    <span>{format!("{} · {} {}", format_datetime(s.created_at), s.item_count, t::ITEMS)}</span>
                    {sheet_badge(s.status)}
                </div>
            </a>
        </li>
    }
}

#[component]
pub fn AdminSheetPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || {
        params
            .read()
            .get("id")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_default()
    };
    let sheet = Resource::new(id, admin_get_sheet);
    view! {
        <a class="back" href="/admin/afregninger">"← "{t::BACK}</a>
        <Transition fallback=loading>
            {move || Suspend::new(async move {
                match sheet.await {
                    Ok(Some(detail)) => view! { <SheetView detail /> }.into_any(),
                    Ok(None) => view! { <p class="empty">{t::SHEET_NOT_FOUND}</p> }.into_any(),
                    Err(e) => view! { <p class="alert">{error_text(&e)}</p> }.into_any(),
                }
            })}
        </Transition>
    }
}

#[component]
pub fn AdminUsersPage() -> impl IntoView {
    let users = Resource::new(|| (), |_| admin_list_users());
    view! {
        <Transition fallback=loading>
            {move || Suspend::new(async move {
                match users.await {
                    Ok(list) if list.is_empty() => view! { <p class="empty">{t::NO_USERS}</p> }.into_any(),
                    Ok(list) => view! { <ul class="expense-list">{list.into_iter().map(user_card).collect_view()}</ul> }.into_any(),
                    Err(e) => view! { <p class="alert">{error_text(&e)}</p> }.into_any(),
                }
            })}
        </Transition>
    }
}

fn user_card(u: AdminUser) -> impl IntoView {
    let provider = if u.provider == "entra" {
        t::PROVIDER_ENTRA
    } else {
        t::PROVIDER_LOCAL
    };
    let login = u.email.clone().unwrap_or_else(|| u.username.clone());
    let last_login = u
        .last_login_at
        .map_or_else(|| t::NEVER.to_string(), format_datetime);
    let id = u.id;
    view! {
        <li class="sheet-card user-card">
            <div class="line">
                <strong class="title">{u.display_name}</strong>
                <span>
                    {(u.role == Role::Admin).then(|| view! { <span class="status status-new">{t::ROLE_ADMIN}</span> })}
                    {u.disabled.then(|| view! { " " <span class="status status-invalid">{t::USER_DISABLED}</span> })}
                </span>
            </div>
            <div class="muted small">{format!("{login} · {provider}")}</div>
            <div class="muted small">{format!("{}: {last_login}", t::LAST_LOGIN)}</div>
            <div class="line small links">
                <a href=format!("/admin?bruger={id}")>{format!("{} {}", u.expense_count, t::ITEMS)}</a>
                <a href=format!("/admin/afregninger?bruger={id}")>
                    {format!("{} {}", u.sheet_count, t::ADMIN_TAB_SHEETS.to_lowercase())}
                </a>
                <a href=format!("/admin/log?bruger={id}")>{t::LOG_FOR_USER}</a>
            </div>
        </li>
    }
}

#[component]
pub fn AdminLogPage() -> impl IntoView {
    let user = query_i64("bruger");
    let before = query_i64("foer");
    let entries = Resource::new(move || (user(), before()), |(u, b)| admin_audit_log(u, b));

    view! {
        <UserSelect
            selected=user()
            to_href=|u| href("/admin/log", &[("bruger", u.map(|u| u.to_string()))])
        />
        <Transition fallback=loading>
            {move || Suspend::new(async move {
                match entries.await {
                    Ok(list) if list.is_empty() => view! { <p class="empty">{t::NO_LOG_ENTRIES}</p> }.into_any(),
                    Ok(list) => {
                        let older = list.last().map(|e| {
                            href(
                                "/admin/log",
                                &[("bruger", user().map(|u| u.to_string())), ("foer", Some(e.id.to_string()))],
                            )
                        });
                        let full_page = list.len() as i64 >= 100;
                        view! {
                            <ol class="log card">{list.into_iter().map(log_entry).collect_view()}</ol>
                            {older
                                .filter(|_| full_page)
                                .map(|h| view! { <a class="button block" href=h>{t::SHOW_OLDER}</a> })}
                        }
                            .into_any()
                    }
                    Err(e) => view! { <p class="alert">{error_text(&e)}</p> }.into_any(),
                }
            })}
        </Transition>
    }
}

fn entity_link(entity_type: Option<&str>, entity_id: Option<&str>) -> Option<AnyView> {
    let id = entity_id?;
    let numeric = id.parse::<i64>().is_ok();
    let (label, path) = match entity_type? {
        "expense" => (t::VOUCHER, "/admin/bilag/"),
        "sheet" => (t::EXPENSE_SHEET, "/admin/afregninger/"),
        "user" if numeric => (t::ADMIN_TAB_USERS, "/admin/log?bruger="),
        // Failed logins record the attempted username, not a user id.
        "user" => return Some(view! { <span>{format!("\u{201c}{id}\u{201d}")}</span> }.into_any()),
        _ => return None,
    };
    Some(view! { <a href=format!("{path}{id}")>{format!("{label} #{id}")}</a> }.into_any())
}

fn log_entry(e: AuditEntry) -> impl IntoView {
    let actor = e
        .actor_name
        .clone()
        .unwrap_or_else(|| t::SYSTEM.to_string());
    let details = e.details.as_deref().map(|d| {
        let short: String = d.chars().take(300).collect();
        if short.len() < d.len() {
            format!("{short}…")
        } else {
            short
        }
    });
    view! {
        <li>
            <div class="line">
                <strong>{audit_action_label(&e.action).to_string()}</strong>
                <span class="muted small">{format_datetime(e.at)}</span>
            </div>
            <div class="line small">
                <span>{actor} " " {entity_link(e.entity_type.as_deref(), e.entity_id.as_deref())}</span>
                <span class="muted">{e.ip}</span>
            </div>
            {details.map(|d| view! { <code class="details">{d}</code> })}
        </li>
    }
}
