use chrono::NaiveDate;
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUser {
    pub id: i64,
    pub display_name: String,
    pub role: Role,
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
}

impl AttachmentRef {
    pub fn url(&self) -> String {
        format!("/filer/{}", self.id)
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
