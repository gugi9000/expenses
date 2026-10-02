use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Admin => "admin",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "user" => Some(Role::User),
            "admin" => Some(Role::Admin),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "system" => Some(Theme::System),
            "light" => Some(Theme::Light),
            "dark" => Some(Theme::Dark),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUser {
    pub id: i64,
    pub display_name: String,
    pub role: Role,
    pub theme: Theme,
}

impl SessionUser {
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpenseStatus {
    Draft,
    New,
    Used,
    Invalid,
    Duplicate,
}

impl ExpenseStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ExpenseStatus::Draft => "draft",
            ExpenseStatus::New => "new",
            ExpenseStatus::Used => "used",
            ExpenseStatus::Invalid => "invalid",
            ExpenseStatus::Duplicate => "duplicate",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(ExpenseStatus::Draft),
            "new" => Some(ExpenseStatus::New),
            "used" => Some(ExpenseStatus::Used),
            "invalid" => Some(ExpenseStatus::Invalid),
            "duplicate" => Some(ExpenseStatus::Duplicate),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        use crate::i18n::t;
        match self {
            ExpenseStatus::Draft => t::STATUS_DRAFT,
            ExpenseStatus::New => t::STATUS_NEW,
            ExpenseStatus::Used => t::STATUS_USED,
            ExpenseStatus::Invalid => t::STATUS_INVALID,
            ExpenseStatus::Duplicate => t::STATUS_DUPLICATE,
        }
    }

    /// Whether a user may move an expense between these states by hand.
    /// `used` is only entered and left through expense sheets.
    pub fn user_can_change_to(self, to: ExpenseStatus) -> bool {
        use ExpenseStatus::*;
        matches!(
            (self, to),
            (Draft | New, Invalid | Duplicate) | (Invalid | Duplicate, Draft | New)
        )
    }

    pub fn is_editable(self) -> bool {
        self != ExpenseStatus::Used
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpenseKind {
    Receipt,
    Bill,
    Invoice,
}

impl ExpenseKind {
    pub const ALL: [ExpenseKind; 3] = [
        ExpenseKind::Receipt,
        ExpenseKind::Bill,
        ExpenseKind::Invoice,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ExpenseKind::Receipt => "receipt",
            ExpenseKind::Bill => "bill",
            ExpenseKind::Invoice => "invoice",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "receipt" => Some(ExpenseKind::Receipt),
            "bill" => Some(ExpenseKind::Bill),
            "invoice" => Some(ExpenseKind::Invoice),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        use crate::i18n::t;
        match self {
            ExpenseKind::Receipt => t::KIND_RECEIPT,
            ExpenseKind::Bill => t::KIND_BILL,
            ExpenseKind::Invoice => t::KIND_INVOICE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SheetStatus {
    Active,
    Voided,
}

impl SheetStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SheetStatus::Active => "active",
            SheetStatus::Voided => "voided",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(SheetStatus::Active),
            "voided" => Some(SheetStatus::Voided),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        use crate::i18n::t;
        match self {
            SheetStatus::Active => t::SHEET_ACTIVE,
            SheetStatus::Voided => t::SHEET_VOIDED,
        }
    }
}

pub const BASE_CURRENCY: &str = "DKK";

/// Currencies offered in the UI; all except DKK must be published by the ECB.
pub const CURRENCIES: &[&str] = &[
    "DKK", "EUR", "SEK", "NOK", "GBP", "USD", "CHF", "PLN", "CZK", "HUF", "ISK", "RON", "TRY",
    "JPY", "CNY", "CAD", "AUD", "NZD", "SGD", "HKD", "KRW", "INR", "THB", "ZAR", "BRL", "MXN",
    "ILS",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentRef {
    pub id: i64,
    pub mime: String,
    pub original_name: Option<String>,
    /// Auto-cropped on upload; the uncropped photo is at `original_url()`.
    pub cropped: bool,
}

impl AttachmentRef {
    pub fn url(&self) -> String {
        format!("/filer/{}", self.id)
    }

    pub fn original_url(&self) -> String {
        format!("/filer/{}/original", self.id)
    }

    pub fn is_image(&self) -> bool {
        matches!(
            self.mime.as_str(),
            "image/jpeg" | "image/png" | "image/webp"
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpenseListItem {
    pub id: i64,
    pub owner_name: String,
    /// Only ever true in the admin view; users never see their deleted expenses.
    pub deleted: bool,
    pub kind: ExpenseKind,
    pub status: ExpenseStatus,
    pub vendor: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub expense_date: Option<NaiveDate>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub amount_base_minor: Option<i64>,
    pub thumbnail: Option<AttachmentRef>,
    pub attachment_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpenseDetail {
    pub id: i64,
    pub owner_name: String,
    pub deleted: bool,
    pub kind: ExpenseKind,
    pub status: ExpenseStatus,
    pub category_id: Option<i64>,
    pub vendor: Option<String>,
    pub description: Option<String>,
    pub expense_date: Option<NaiveDate>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub fx_rate: Option<String>,
    pub fx_rate_date: Option<NaiveDate>,
    pub amount_base_minor: Option<i64>,
    pub attachments: Vec<AttachmentRef>,
    /// Other expenses of the same owner that contain an identical file.
    pub duplicates_of: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadResponse {
    pub expense_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetSummary {
    pub id: i64,
    pub owner_name: String,
    pub title: String,
    pub status: SheetStatus,
    pub created_at: DateTime<Utc>,
    pub item_count: i64,
    pub total_base_minor: i64,
}

/// A snapshot of an expense as it was when the sheet was created.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetItem {
    pub position: usize,
    pub expense_id: i64,
    pub kind: ExpenseKind,
    pub category: Option<String>,
    pub vendor: Option<String>,
    pub description: Option<String>,
    pub expense_date: NaiveDate,
    pub amount_minor: i64,
    pub currency: String,
    pub fx_rate: String,
    pub amount_base_minor: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetDetail {
    pub summary: SheetSummary,
    pub owner_name: String,
    pub bank_account: Option<String>,
    pub items: Vec<SheetItem>,
}

impl SheetDetail {
    pub fn pdf_url(&self) -> String {
        format!("/afregninger/{}/pdf", self.summary.id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetDefaults {
    pub title: String,
    pub bank_account: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminUser {
    pub id: i64,
    pub display_name: String,
    pub username: String,
    pub email: Option<String>,
    pub provider: String,
    pub role: Role,
    pub disabled: bool,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub expense_count: i64,
    pub sheet_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub at: DateTime<Utc>,
    pub actor_id: Option<i64>,
    pub actor_name: Option<String>,
    pub action: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub details: Option<String>,
    pub ip: Option<String>,
}

/// Totals per category, largest first, for sheet summaries.
pub fn totals_by_category<'a>(
    items: impl IntoIterator<Item = (Option<&'a str>, i64)>,
) -> Vec<(String, i64)> {
    let mut totals: Vec<(String, i64)> = Vec::new();
    for (category, amount) in items {
        let name = category.unwrap_or("–");
        match totals.iter_mut().find(|(n, _)| n == name) {
            Some((_, sum)) => *sum += amount,
            None => totals.push((name.to_string(), amount)),
        }
    }
    totals.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    totals
}

#[cfg(test)]
mod tests {
    use super::ExpenseStatus::*;

    #[test]
    fn manual_transitions() {
        assert!(Draft.user_can_change_to(Invalid));
        assert!(New.user_can_change_to(Duplicate));
        assert!(Invalid.user_can_change_to(New));
        assert!(Duplicate.user_can_change_to(Draft));
        assert!(!New.user_can_change_to(Used));
        assert!(!Used.user_can_change_to(New));
        assert!(!Used.user_can_change_to(Invalid));
        assert!(!Draft.user_can_change_to(New));
        assert!(!Used.is_editable());
        assert!(Invalid.is_editable());
    }
}
