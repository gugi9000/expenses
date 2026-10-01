use leptos::{prelude::*, task::spawn_local};
use leptos_router::hooks::{use_navigate, use_params_map, use_query_map};

use crate::{
    api::{
        ChangeExpenseStatus, DeleteExpense, UpdateExpense, error_text, get_expense,
        list_categories, list_expenses,
    },
    i18n::{format_amount, format_date, format_number, format_rate, t},
    model::{
        AttachmentRef, BASE_CURRENCY, CURRENCIES, Category, ExpenseDetail, ExpenseKind,
        ExpenseListItem, ExpenseStatus,
    },
};

pub const STATUS_FILTERS: [Option<ExpenseStatus>; 6] = [
    None,
    Some(ExpenseStatus::Draft),
    Some(ExpenseStatus::New),
    Some(ExpenseStatus::Used),
    Some(ExpenseStatus::Invalid),
    Some(ExpenseStatus::Duplicate),
];

fn list_href(status: Option<&str>, category: Option<i64>) -> String {
    let mut params = Vec::new();
    if let Some(s) = status {
        params.push(format!("status={s}"));
    }
    if let Some(c) = category {
        params.push(format!("kategori={c}"));
    }
    if params.is_empty() {
        "/".into()
    } else {
        format!("/?{}", params.join("&"))
    }
}

#[component]
pub fn UploadButtons(
    expense_id: Option<i64>,
    #[prop(into)] on_done: Callback<i64>,
) -> impl IntoView {
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let on_change = move |ev: leptos::ev::Event| {
        let input: web_sys::HtmlInputElement = event_target(&ev);
        let Some(list) = input.files() else { return };
        let files: Vec<web_sys::File> = (0..list.length()).filter_map(|i| list.get(i)).collect();
        input.set_value("");
        if files.is_empty() {
            return;
        }
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            let result = crate::upload::upload(files, expense_id).await;
            busy.set(false);
            match result {
                Ok(id) => on_done.run(id),
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let camera_label = if expense_id.is_some() {
        t::ADD_PAGE
    } else {
        t::TAKE_PHOTO
    };
    view! {
        <div class="upload">
            <div class="upload-buttons">
                <label class="button primary" class:disabled=busy>
                    {camera_label}
                    <input
                        type="file"
                        accept="image/*"
                        capture="environment"
                        class="visually-hidden"
                        disabled=busy
                        on:change=on_change
                    />
                </label>
                <label class="button" class:disabled=busy>
                    {t::CHOOSE_FILE}
                    <input
                        type="file"
                        accept="image/*,application/pdf"
                        multiple
                        class="visually-hidden"
                        disabled=busy
                        on:change=on_change
                    />
                </label>
            </div>
            <Show when=move || busy.get()>
                <p class="muted" role="status">{t::UPLOADING}</p>
            </Show>
            {move || error.get().map(|e| view! { <p class="alert" role="alert">{e}</p> })}
        </div>
    }
}

#[component]
pub fn ExpenseListPage() -> impl IntoView {
    let query = use_query_map();
    let status = move || {
        query
            .read()
            .get("status")
            .filter(|s| ExpenseStatus::parse(s).is_some())
    };
    let category = move || {
        query
            .read()
            .get("kategori")
            .and_then(|c| c.parse::<i64>().ok())
    };
    let expenses = Resource::new(move || (status(), category()), |(s, c)| list_expenses(s, c));
    let categories = Resource::new(|| (), |_| list_categories());

    let navigate = use_navigate();
    let on_uploaded = Callback::new({
        let navigate = navigate.clone();
        move |id: i64| navigate(&format!("/bilag/{id}"), Default::default())
    });
    let on_category = move |ev: leptos::ev::Event| {
        let value = event_target_value(&ev);
        navigate(
            &list_href(status().as_deref(), value.parse().ok()),
            Default::default(),
        );
    };

    view! {
        <h1>{t::MY_VOUCHERS}</h1>
        <UploadButtons expense_id=None on_done=on_uploaded />

        <nav class="chips" aria-label=t::FIELD_KIND>
            {STATUS_FILTERS
                .into_iter()
                .map(|f| {
                    let code = f.map(|s| s.as_str());
                    let label = f.map_or(t::FILTER_ALL, |s| s.label());
                    view! {
                        <a
                            class="chip"
                            class:active=move || status().as_deref() == code
                            href=move || list_href(code, category())
                        >
                            {label}
                        </a>
                    }
                })
                .collect_view()}
        </nav>

        <Suspense>
            {move || {
                let on_category = on_category.clone();
                Suspend::new(async move {
                let cats = categories.await.unwrap_or_default();
                let selected = category();
                view! {
                    <select class="filter" aria-label=t::FIELD_CATEGORY on:change=on_category>
                        <option value="" selected=selected.is_none()>{t::FILTER_CATEGORY_ALL}</option>
                        {cats
                            .into_iter()
                            .map(|c| {
                                view! {
                                    <option value=c.id.to_string() selected=selected == Some(c.id)>{c.name}</option>
                                }
                            })
                            .collect_view()}
                    </select>
                }
            })
            }}
        </Suspense>

        <Transition fallback=|| view! { <p class="page-message">{t::LOADING}</p> }>
            {move || Suspend::new(async move {
                let filtered = status().is_some() || category().is_some();
                match expenses.await {
                    Ok(items) if items.is_empty() => {
                        view! {
                            <p class="empty">{if filtered { t::NO_MATCHING } else { t::NO_EXPENSES_YET }}</p>
                        }
                            .into_any()
                    }
                    Ok(items) => {
                        view! {
                            <ul class="expense-list">
                                {items.into_iter().map(|item| view! { <ExpenseCard item /> }).collect_view()}
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

fn thumbnail(att: Option<AttachmentRef>) -> impl IntoView {
    match att {
        Some(a) if a.is_image() => view! { <img src=a.url() alt="" loading="lazy" /> }.into_any(),
        Some(_) => view! { <span class="file-badge">{t::PDF}</span> }.into_any(),
        None => ().into_any(),
    }
}

pub fn status_badge(status: ExpenseStatus) -> impl IntoView {
    view! { <span class=format!("status status-{}", status.as_str())>{status.label()}</span> }
}

/// Thumbnail and details of a voucher, used inside list links and selection rows.
pub fn expense_card_body(item: ExpenseListItem, show_owner: bool) -> impl IntoView {
    let owner = show_owner.then(|| {
        let deleted = item.deleted.then(|| format!(" · {}", t::DELETED));
        view! { <div class="line owner small"><span>{item.owner_name.clone()}{deleted}</span></div> }
    });
    let title = item
        .vendor
        .clone()
        .unwrap_or_else(|| item.kind.label().to_string());
    let amount = item
        .amount_minor
        .zip(item.currency.clone())
        .map(|(a, c)| format_amount(a, &c));
    let base = (item.currency.as_deref() != Some(BASE_CURRENCY))
        .then_some(item.amount_base_minor)
        .flatten()
        .map(|b| format!("≈ {}", format_amount(b, BASE_CURRENCY)));
    let date = item.expense_date.map(format_date);
    let pages =
        (item.attachment_count > 1).then(|| format!("{} {}", item.attachment_count, t::PAGES));

    view! {
        <div class="thumb">{thumbnail(item.thumbnail)}</div>
        <div class="info">
            {owner}
            <div class="line">
                <strong class="title">{title}</strong>
                <span class="amount">{amount}</span>
            </div>
            <div class="line muted">
                <span>{date.unwrap_or_else(|| t::MISSING_DETAILS.to_string())}</span>
                <span class="small">{base}</span>
            </div>
            <div class="line">
                <span class="muted small">
                    {item.category}
                    {pages.map(|p| format!(" · {p}"))}
                </span>
                {status_badge(item.status)}
            </div>
        </div>
    }
}

#[component]
fn ExpenseCard(item: ExpenseListItem) -> impl IntoView {
    let href = format!("/bilag/{}", item.id);
    view! {
        <li>
            <a class="expense-card" href=href>
                {expense_card_body(item, false)}
            </a>
        </li>
    }
}

#[component]
pub fn ExpenseDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || {
        params
            .read()
            .get("id")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_default()
    };

    let update = ServerAction::<UpdateExpense>::new();
    let status_action = ServerAction::<ChangeExpenseStatus>::new();
    let delete = ServerAction::<DeleteExpense>::new();

    // Only refetch after successful changes, so a failed save keeps the user's input on screen.
    let refresh = RwSignal::new(0u32);
    Effect::new(move |_| {
        let ok = matches!(update.value().get(), Some(Ok(())))
            || matches!(status_action.value().get(), Some(Ok(())));
        if ok {
            refresh.update(|n| *n += 1);
        }
    });
    let navigate = use_navigate();
    Effect::new(move |_| {
        if matches!(delete.value().get(), Some(Ok(()))) {
            navigate("/", Default::default());
        }
    });

    let expense = Resource::new(move || (id(), refresh.get()), |(id, _)| get_expense(id));
    let categories = Resource::new(|| (), |_| list_categories());
    let on_pages_added = Callback::new(move |_: i64| refresh.update(|n| *n += 1));

    view! {
        <a class="back" href="/">"← "{t::BACK}</a>
        <Transition fallback=|| view! { <p class="page-message">{t::LOADING}</p> }>
            {move || Suspend::new(async move {
                let cats = categories.await.unwrap_or_default();
                match expense.await {
                    Ok(Some(expense)) => {
                        view! {
                            <ExpenseEditor
                                expense
                                categories=cats
                                update
                                status_action
                                delete
                                on_pages_added
                            />
                        }
                            .into_any()
                    }
                    Ok(None) => view! { <p class="empty">{t::VOUCHER_NOT_FOUND}</p> }.into_any(),
                    Err(e) => view! { <p class="alert">{error_text(&e)}</p> }.into_any(),
                }
            })}
        </Transition>
    }
}

pub fn attachment_view(a: AttachmentRef) -> impl IntoView {
    let url = a.url();
    if a.is_image() {
        let href = url.clone();
        view! {
            <a class="page" href=href target="_blank" rel="noopener external">
                <img src=url alt="" />
            </a>
        }
        .into_any()
    } else {
        let name = a.original_name.unwrap_or_else(|| t::PDF.to_string());
        view! {
            <a class="page file" href=url target="_blank" rel="noopener external">
                <span class="file-badge">{t::PDF}</span>
                <span>{t::OPEN_FILE}": "{name}</span>
            </a>
        }
        .into_any()
    }
}

#[component]
fn ExpenseEditor(
    expense: ExpenseDetail,
    categories: Vec<Category>,
    update: ServerAction<UpdateExpense>,
    status_action: ServerAction<ChangeExpenseStatus>,
    delete: ServerAction<DeleteExpense>,
    on_pages_added: Callback<i64>,
) -> impl IntoView {
    let id = expense.id;
    let status = expense.status;
    let editable = status.is_editable();
    let currency = expense
        .currency
        .clone()
        .unwrap_or_else(|| BASE_CURRENCY.to_string());
    let amount = expense
        .amount_minor
        .map(|a| format_number(a, &currency))
        .unwrap_or_default();
    let date = expense
        .expense_date
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let converted = (currency != BASE_CURRENCY)
        .then(|| {
            expense
                .amount_base_minor
                .zip(expense.fx_rate.clone())
                .zip(expense.fx_rate_date)
        })
        .flatten()
        .map(|((base, rate), rate_date)| {
            format!(
                "{}: {} ({} {}: {})",
                t::CONVERTED,
                format_amount(base, BASE_CURRENCY),
                t::FX_RATE_FROM,
                format_date(rate_date),
                format_rate(&rate)
            )
        });

    let status_button = move |target: ExpenseStatus, label: &'static str| {
        view! {
            <ActionForm action=status_action>
                <input type="hidden" name="id" value=id />
                <input type="hidden" name="status" value=target.as_str() />
                <button class="button" type="submit">{label}</button>
            </ActionForm>
        }
    };
    let on_delete = move |_| {
        let confirmed = web_sys::window()
            .and_then(|w| w.confirm_with_message(t::CONFIRM_DELETE).ok())
            .unwrap_or(false);
        if confirmed {
            delete.dispatch(DeleteExpense { id });
        }
    };
    let action_error = move || {
        [status_action.value().get(), delete.value().get()]
            .into_iter()
            .flatten()
            .find_map(|r| r.err())
            .map(|e| view! { <p class="alert" role="alert">{error_text(&e)}</p> })
    };

    view! {
        <div class="detail-head">
            <h1>{expense.vendor.clone().unwrap_or_else(|| expense.kind.label().to_string())}</h1>
            {status_badge(status)}
        </div>

        {(!editable).then(|| view! { <p class="notice">{t::LOCKED_USED}</p> })}
        {(!expense.duplicates_of.is_empty())
            .then(|| {
                view! {
                    <p class="alert">
                        {t::DUPLICATE_WARNING}" "
                        {expense
                            .duplicates_of
                            .iter()
                            .map(|d| view! { <a href=format!("/bilag/{d}")>{format!("#{d}")}</a>" " })
                            .collect_view()}
                    </p>
                }
            })}

        <div class="pages">{expense.attachments.into_iter().map(attachment_view).collect_view()}</div>
        {editable.then(|| view! { <UploadButtons expense_id=Some(id) on_done=on_pages_added /> })}

        <ActionForm action=update attr:class="card form">
            <fieldset disabled=!editable>
                <input type="hidden" name="id" value=id />
                <label>
                    {t::FIELD_KIND}
                    <select name="kind">
                        {ExpenseKind::ALL
                            .into_iter()
                            .map(|k| {
                                view! {
                                    <option value=k.as_str() selected=k == expense.kind>{k.label()}</option>
                                }
                            })
                            .collect_view()}
                    </select>
                </label>
                <label>
                    {t::FIELD_CATEGORY}
                    <select name="category_id">
                        <option value="" selected=expense.category_id.is_none()>{t::CHOOSE}</option>
                        {categories
                            .into_iter()
                            .map(|c| {
                                view! {
                                    <option value=c.id.to_string() selected=expense.category_id == Some(c.id)>
                                        {c.name}
                                    </option>
                                }
                            })
                            .collect_view()}
                    </select>
                </label>
                <label>
                    {t::FIELD_VENDOR}
                    <input name="vendor" maxlength="200" autocomplete="off" value=expense.vendor.clone() />
                </label>
                <label>
                    {t::FIELD_DATE}
                    <input type="date" name="expense_date" value=date />
                </label>
                <div class="field-row">
                    <label class="grow">
                        {t::FIELD_AMOUNT}
                        <input
                            name="amount"
                            inputmode="decimal"
                            autocomplete="off"
                            placeholder=t::AMOUNT_PLACEHOLDER
                            value=amount
                        />
                    </label>
                    <label>
                        {t::FIELD_CURRENCY}
                        <select name="currency">
                            {CURRENCIES
                                .iter()
                                .map(|c| view! { <option value=*c selected=*c == currency>{*c}</option> })
                                .collect_view()}
                        </select>
                    </label>
                </div>
                {converted.map(|c| view! { <p class="muted small">{c}</p> })}
                <label>
                    {t::FIELD_DESCRIPTION}
                    <textarea name="description" rows="3" maxlength="1000">
                        {expense.description.clone().unwrap_or_default()}
                    </textarea>
                </label>
                {move || {
                    update
                        .value()
                        .get()
                        .map(|r| match r {
                            Ok(()) => view! { <p class="success" role="status">{t::SAVED}</p> }.into_any(),
                            Err(e) => view! { <p class="alert" role="alert">{error_text(&e)}</p> }.into_any(),
                        })
                }}
                <button class="button primary block" type="submit" disabled=move || update.pending().get()>
                    {move || if update.pending().get() { t::SAVING } else { t::SAVE }}
                </button>
            </fieldset>
        </ActionForm>

        {editable
            .then(|| {
                view! {
                    <div class="actions">
                        {match status {
                            ExpenseStatus::Draft | ExpenseStatus::New => {
                                view! {
                                    {status_button(ExpenseStatus::Invalid, t::MARK_INVALID)}
                                    {status_button(ExpenseStatus::Duplicate, t::MARK_DUPLICATE)}
                                }
                                    .into_any()
                            }
                            _ => status_button(ExpenseStatus::New, t::RESTORE).into_any(),
                        }}
                        <button class="button danger" type="button" on:click=on_delete>{t::DELETE}</button>
                    </div>
                    {action_error}
                }
            })}
    }
}
