use std::collections::HashSet;

use leptos::{ev::SubmitEvent, prelude::*};
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::{
    api::{CreateSheet, VoidSheet, new_sheet_defaults, error_text, get_sheet, list_expenses, list_sheets},
    i18n::{format_amount, format_date, format_datetime, t},
    model::{BASE_CURRENCY, SheetDetail, SheetStatus, SheetSummary, totals_by_category},
    pages::expense_card_body,
};

pub fn sheet_badge(status: SheetStatus) -> impl IntoView {
    view! { <span class=format!("status status-sheet-{}", status.as_str())>{status.label()}</span> }
}

fn totals_view(rows: Vec<(String, i64)>, total: i64) -> impl IntoView {
    view! {
        <div class="totals">
            <h2>{t::BY_CATEGORY}</h2>
            {rows
                .into_iter()
                .map(|(name, sum)| {
                    view! {
                        <div class="line">
                            <span>{name}</span>
                            <span>{format_amount(sum, BASE_CURRENCY)}</span>
                        </div>
                    }
                })
                .collect_view()}
            <div class="line total">
                <span>{t::TOTAL}</span>
                <span>{format_amount(total, BASE_CURRENCY)}</span>
            </div>
        </div>
    }
}

#[component]
pub fn SheetListPage() -> impl IntoView {
    let sheets = Resource::new(|| (), |_| list_sheets());
    view! {
        <h1>{t::MY_SHEETS}</h1>
        <a class="button primary block" href="/afregninger/ny">{t::NEW_SHEET}</a>
        <Transition fallback=|| view! { <p class="page-message">{t::LOADING}</p> }>
            {move || Suspend::new(async move {
                match sheets.await {
                    Ok(list) if list.is_empty() => view! { <p class="empty">{t::NO_SHEETS}</p> }.into_any(),
                    Ok(list) => {
                        view! {
                            <ul class="expense-list sheet-list">
                                {list.into_iter().map(sheet_card).collect_view()}
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

fn sheet_card(s: SheetSummary) -> impl IntoView {
    view! {
        <li>
            <a class="sheet-card" href=format!("/afregninger/{}", s.id)>
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
pub fn NewSheetPage() -> impl IntoView {
    let candidates = Resource::new(|| (), |_| list_expenses(Some("new".into()), None));
    let defaults = Resource::new(|| (), |_| new_sheet_defaults());
    // Tracking what is *deselected* makes "everything selected" the default without extra setup.
    let deselected = RwSignal::new(HashSet::<i64>::new());
    let title = RwSignal::new(String::new());
    let create = ServerAction::<CreateSheet>::new();

    let navigate = use_navigate();
    Effect::new(move |_| {
        if let Some(Ok(id)) = create.value().get() {
            navigate(&format!("/afregninger/{id}"), Default::default());
        }
    });

    view! {
        <a class="back" href="/afregninger">"← "{t::BACK}</a>
        <h1>{t::NEW_SHEET}</h1>
        <Suspense fallback=|| view! { <p class="page-message">{t::LOADING}</p> }>
            {move || Suspend::new(async move {
                let defaults = defaults.await.ok();
                let placeholder = defaults.as_ref().map(|d| d.title.clone()).unwrap_or_default();
                let account = RwSignal::new(defaults.and_then(|d| d.bank_account).unwrap_or_default());
                let items = match candidates.await {
                    Ok(items) if items.is_empty() => {
                        return view! { <p class="empty">{t::NO_NEW_EXPENSES}</p> }.into_any();
                    }
                    Ok(items) => items,
                    Err(e) => return view! { <p class="alert">{error_text(&e)}</p> }.into_any(),
                };
                let amounts = StoredValue::new(
                    items
                        .iter()
                        .map(|i| (i.id, i.category.clone(), i.amount_base_minor.unwrap_or(0)))
                        .collect::<Vec<_>>(),
                );
                let is_selected = move |id: i64| !deselected.read().contains(&id);
                let selected_ids = move || {
                    amounts.with_value(|a| a.iter().map(|(id, ..)| *id).filter(|id| is_selected(*id)).collect::<Vec<_>>())
                };
                let count = move || selected_ids().len();
                let totals = move || {
                    amounts.with_value(|a| {
                        let chosen: Vec<_> = a.iter().filter(|(id, ..)| is_selected(*id)).collect();
                        let total = chosen.iter().map(|(.., x)| *x).sum::<i64>();
                        totals_view(totals_by_category(chosen.iter().map(|(_, c, x)| (c.as_deref(), *x))), total)
                    })
                };
                let select_all = move |_| deselected.set(HashSet::new());
                let select_none = move |_| {
                    deselected.set(amounts.with_value(|a| a.iter().map(|(id, ..)| *id).collect()));
                };
                let submit = move |ev: SubmitEvent| {
                    ev.prevent_default();
                    create.dispatch(CreateSheet {
                        title: title.get_untracked(),
                        bank_account: account.get_untracked(),
                        expense_ids: selected_ids(),
                    });
                };

                view! {
                    <form class="form" on:submit=submit>
                        <label>
                            {t::SHEET_TITLE}
                            <input
                                maxlength="120"
                                placeholder=placeholder
                                prop:value=move || title.get()
                                on:input=move |ev| title.set(event_target_value(&ev))
                            />
                        </label>
                        <label>
                            {t::BANK_ACCOUNT}
                            <input
                                maxlength="40"
                                inputmode="text"
                                autocomplete="off"
                                autocapitalize="characters"
                                placeholder=t::BANK_ACCOUNT_PLACEHOLDER
                                prop:value=move || account.get()
                                on:input=move |ev| account.set(event_target_value(&ev))
                            />
                            <span class="hint">{t::BANK_ACCOUNT_HINT}</span>
                        </label>
                        <div class="select-bar">
                            <span>{move || format!("{} {} {}", count(), t::ITEMS, t::SELECTED)}</span>
                            <span>
                                <button type="button" class="link" on:click=select_all>{t::SELECT_ALL}</button>
                                " · "
                                <button type="button" class="link" on:click=select_none>{t::SELECT_NONE}</button>
                            </span>
                        </div>
                        <ul class="expense-list">
                            {items
                                .into_iter()
                                .map(|item| {
                                    let id = item.id;
                                    view! {
                                        <li>
                                            <label class="expense-card pick" class:unselected=move || !is_selected(id)>
                                                <input
                                                    type="checkbox"
                                                    prop:checked=move || is_selected(id)
                                                    on:change=move |_| {
                                                        deselected
                                                            .update(|s| {
                                                                if !s.remove(&id) {
                                                                    s.insert(id);
                                                                }
                                                            })
                                                    }
                                                />
                                                {expense_card_body(item, false)}
                                            </label>
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ul>
                        <div class="card">{totals}</div>
                        {move || {
                            create
                                .value()
                                .get()
                                .and_then(|r| r.err())
                                .map(|e| view! { <p class="alert" role="alert">{error_text(&e)}</p> })
                        }}
                        <button
                            class="button primary block"
                            type="submit"
                            disabled=move || create.pending().get() || count() == 0
                        >
                            {move || if create.pending().get() { t::CREATING } else { t::CREATE_SHEET }}
                        </button>
                    </form>
                }
                    .into_any()
            })}
        </Suspense>
    }
}

#[component]
pub fn SheetDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.read().get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or_default();
    let void = ServerAction::<VoidSheet>::new();
    let sheet = Resource::new(move || (id(), void.version().get()), |(id, _)| get_sheet(id));

    view! {
        <a class="back" href="/afregninger">"← "{t::BACK}</a>
        <Transition fallback=|| view! { <p class="page-message">{t::LOADING}</p> }>
            {move || Suspend::new(async move {
                match sheet.await {
                    Ok(Some(detail)) => view! { <SheetView detail void /> }.into_any(),
                    Ok(None) => view! { <p class="empty">{t::SHEET_NOT_FOUND}</p> }.into_any(),
                    Err(e) => view! { <p class="alert">{error_text(&e)}</p> }.into_any(),
                }
            })}
        </Transition>
    }
}

/// Without `void` the view is read-only and links to the admin pages.
#[component]
pub fn SheetView(detail: SheetDetail, #[prop(optional)] void: Option<ServerAction<VoidSheet>>) -> impl IntoView {
    let id = detail.summary.id;
    let admin = void.is_none();
    let expense_href = move |expense_id: i64| {
        if admin { format!("/admin/bilag/{expense_id}") } else { format!("/bilag/{expense_id}") }
    };
    let active = detail.summary.status == SheetStatus::Active;
    let totals = totals_by_category(detail.items.iter().map(|i| (i.category.as_deref(), i.amount_base_minor)));
    let on_void = move |_| {
        let confirmed = web_sys::window().and_then(|w| w.confirm_with_message(t::CONFIRM_VOID).ok()).unwrap_or(false);
        if let (true, Some(void)) = (confirmed, void) {
            void.dispatch(VoidSheet { id });
        }
    };

    view! {
        <div class="detail-head">
            <h1>{detail.summary.title.clone()}</h1>
            {sheet_badge(detail.summary.status)}
        </div>
        {admin.then(|| view! { <p><strong>{format!("{}: {}", t::OWNER, detail.owner_name)}</strong></p> })}
        <p class="muted">
            {format!(
                "{} #{} · {} {} · {} {}",
                t::PDF_SHEET_NO,
                id,
                t::CREATED,
                format_datetime(detail.summary.created_at),
                detail.items.len(),
                t::ITEMS,
            )}
        </p>
        {detail
            .bank_account
            .clone()
            .map(|a| view! { <p class="muted">{format!("{}: {a}", t::BANK_ACCOUNT)}</p> })}
        {(!active).then(|| view! { <p class="notice">{t::SHEET_VOIDED_NOTICE}</p> })}

        <a class="button primary block" href=detail.pdf_url() rel="external" download>
            {t::DOWNLOAD_PDF}
        </a>

        <ol class="sheet-items card">
            {detail
                .items
                .into_iter()
                .map(|item| {
                    let label = [item.vendor.clone(), item.description.clone()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" – ");
                    let label = if label.is_empty() { item.kind.label().to_string() } else { label };
                    let original = (item.currency != BASE_CURRENCY)
                        .then(|| format_amount(item.amount_minor, &item.currency));
                    view! {
                        <li>
                            <a href=expense_href(item.expense_id)>
                                <div class="line">
                                    <span class="title">{format!("{}. {}", item.position, label)}</span>
                                    <strong class="amount">
                                        {format_amount(item.amount_base_minor, BASE_CURRENCY)}
                                    </strong>
                                </div>
                                <div class="line muted small">
                                    <span>
                                        {format_date(item.expense_date)}
                                        {item.category.map(|c| format!(" · {c}"))}
                                    </span>
                                    <span>{original}</span>
                                </div>
                            </a>
                        </li>
                    }
                })
                .collect_view()}
        </ol>

        <div class="card">{totals_view(totals, detail.summary.total_base_minor)}</div>

        {void.filter(|_| active)
            .map(|void| {
                view! {
                    <div class="actions">
                        <button
                            class="button danger"
                            type="button"
                            disabled=move || void.pending().get()
                            on:click=on_void
                        >
                            {t::VOID_SHEET}
                        </button>
                    </div>
                }
            })}
        {move || {
            void.and_then(|void| void.value().get())
                .and_then(|r| r.err())
                .map(|e| view! { <p class="alert" role="alert">{error_text(&e)}</p> })
        }}
    }
}
