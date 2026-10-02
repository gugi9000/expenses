//! Danish UI text and da-DK formatting. Strings are constants so a missing key is a compile error.

use chrono::{DateTime, Datelike, Days, Duration, NaiveDate, NaiveDateTime, Utc};

pub mod t {
    pub const APP_NAME: &str = "Udgifter";
    pub const FOOTER_BRAND: &str = "Fair IT Expenses";
    pub const UPDATE_AVAILABLE: &str = "Der er en ny version af appen.";
    pub const UPDATE_RELOAD: &str = "Genindlæs";
    pub const NOT_FOUND: &str = "Siden blev ikke fundet.";
    pub const LOADING: &str = "Indlæser …";
    pub const GENERIC_ERROR: &str = "Der opstod en fejl. Prøv igen.";

    pub const LOGIN_TITLE: &str = "Log ind";
    pub const LOGIN_WITH_MICROSOFT: &str = "Log ind med Microsoft";
    pub const LOGIN_LOCAL_HEADING: &str = "Lokal bruger";
    pub const USERNAME: &str = "Brugernavn";
    pub const PASSWORD: &str = "Adgangskode";
    pub const LOGIN_SUBMIT: &str = "Log ind";
    pub const LOGIN_FAILED: &str = "Forkert brugernavn eller adgangskode.";
    pub const LOGIN_RATE_LIMITED: &str = "For mange forsøg. Vent et øjeblik, og prøv igen.";
    pub const LOGIN_ENTRA_FAILED: &str = "Log ind med Microsoft mislykkedes. Prøv igen.";
    pub const LOGIN_DISABLED: &str = "Din bruger er deaktiveret.";
    pub const LOGOUT: &str = "Log ud";
    pub const LOGGED_IN_AS: &str = "Logget ind som";
    pub const OR: &str = "eller";

    pub const GREETING: &str = "Hej";
    pub const NAV_EXPENSES: &str = "Bilag";
    pub const NAV_SHEETS: &str = "Afregninger";
    pub const NAV_ADMIN: &str = "Administration";
    pub const ADD_VOUCHER: &str = "Tilføj bilag";
    pub const NO_EXPENSES_YET: &str = "Du har ingen bilag endnu.";
    pub const ROLE_ADMIN: &str = "Administrator";
    pub const ROLE_USER: &str = "Bruger";

    pub const VOUCHER: &str = "Bilag";
    pub const EXPENSE: &str = "Udgift";
    pub const EXPENSE_SHEET: &str = "Udgiftsafregning";

    pub const KIND_RECEIPT: &str = "Kvittering";
    pub const KIND_BILL: &str = "Regning";
    pub const KIND_INVOICE: &str = "Faktura";

    pub const STATUS_DRAFT: &str = "Kladde";
    pub const STATUS_NEW: &str = "Ny";
    pub const STATUS_USED: &str = "Brugt";
    pub const STATUS_INVALID: &str = "Ugyldig";
    pub const STATUS_DUPLICATE: &str = "Dublet";

    pub const SHEET_ACTIVE: &str = "Aktiv";
    pub const SHEET_VOIDED: &str = "Annulleret";

    pub const MY_VOUCHERS: &str = "Mine bilag";
    pub const TAKE_PHOTO: &str = "Tag billede";
    pub const CHOOSE_FILE: &str = "Vælg fil";
    pub const ADD_PAGE: &str = "Tilføj side";
    pub const AUTO_CROP: &str = "Beskær automatisk";
    pub const AUTO_CROP_HINT: &str =
        "Finder papirets kanter, retter det op og skærer baggrunden fra. Gælder ikke PDF.";
    pub const SHOW_ORIGINAL: &str = "Vis original";
    pub const UPLOADING: &str = "Uploader …";
    pub const UPLOAD_FAILED: &str = "Upload mislykkedes. Prøv igen.";
    pub const FILTER_ALL: &str = "Alle";
    pub const FILTER_CATEGORY_ALL: &str = "Alle udgiftstyper";
    pub const NO_MATCHING: &str = "Ingen bilag matcher filteret.";
    pub const MISSING_DETAILS: &str = "Mangler oplysninger";
    pub const PAGES: &str = "sider";
    pub const PDF: &str = "PDF";
    pub const OPEN_FILE: &str = "Åbn fil";
    pub const BACK: &str = "Tilbage";
    pub const VOUCHER_NOT_FOUND: &str = "Bilaget findes ikke.";

    pub const FIELD_KIND: &str = "Type";
    pub const FIELD_CATEGORY: &str = "Udgiftstype";
    pub const FIELD_VENDOR: &str = "Forretning / leverandør";
    pub const FIELD_DESCRIPTION: &str = "Beskrivelse";
    pub const FIELD_DATE: &str = "Dato";
    pub const FIELD_AMOUNT: &str = "Beløb";
    pub const FIELD_CURRENCY: &str = "Valuta";
    pub const CHOOSE: &str = "Vælg …";
    pub const AMOUNT_PLACEHOLDER: &str = "fx 1.234,50";
    pub const SAVE: &str = "Gem";
    pub const SAVING: &str = "Gemmer …";
    pub const SAVED: &str = "Gemt.";
    pub const CONVERTED: &str = "Omregnet";
    pub const FX_RATE_FROM: &str = "ECB-kurs fra";

    pub const MARK_INVALID: &str = "Markér som ugyldig";
    pub const MARK_DUPLICATE: &str = "Markér som dublet";
    pub const RESTORE: &str = "Gendan";
    pub const DELETE: &str = "Slet";
    pub const CONFIRM_DELETE: &str = "Vil du slette bilaget?";
    pub const LOCKED_USED: &str = "Bilaget er brugt i en udgiftsafregning og kan ikke ændres.";
    pub const DUPLICATE_WARNING: &str = "Samme fil findes også på bilag";

    pub const ERR_NOT_LOGGED_IN: &str = "Du er ikke logget ind.";
    pub const ERR_INVALID_KIND: &str = "Ugyldig bilagstype.";
    pub const ERR_INVALID_CATEGORY: &str = "Ugyldig udgiftstype.";
    pub const ERR_INVALID_DATE: &str = "Ugyldig dato.";
    pub const ERR_FUTURE_DATE: &str = "Datoen må ikke ligge i fremtiden.";
    pub const ERR_INVALID_AMOUNT: &str = "Ugyldigt beløb. Skriv fx 1.234,50.";
    pub const ERR_INVALID_CURRENCY: &str = "Ukendt valuta.";
    pub const ERR_FX_MISSING: &str =
        "Der findes endnu ingen valutakurs for den valgte valuta og dato.";
    pub const ERR_TOO_LONG: &str = "Teksten er for lang.";
    pub const ERR_STATUS_CHANGE: &str = "Statusændringen er ikke tilladt.";
    pub const ERR_FILE_TYPE: &str = "Filtypen understøttes ikke. Brug JPEG, PNG, HEIC eller PDF.";
    pub const ERR_FILE_TOO_LARGE: &str = "Filen er for stor (maks. 20 MB).";
    pub const ERR_NO_FILES: &str = "Vælg mindst én fil.";
    pub const ERR_TOO_MANY_FILES: &str = "For mange filer på én gang (maks. 10).";

    pub const MY_SHEETS: &str = "Mine afregninger";
    pub const NEW_SHEET: &str = "Ny afregning";
    pub const SHEET_TITLE: &str = "Titel";
    pub const SELECT_ALL: &str = "Vælg alle";
    pub const SELECT_NONE: &str = "Fravælg alle";
    pub const SELECTED: &str = "valgt";
    pub const CREATE_SHEET: &str = "Opret afregning";
    pub const CREATING: &str = "Opretter …";
    pub const NO_NEW_EXPENSES: &str = "Du har ingen nye bilag at afregne. Et bilag skal have udgiftstype, dato og beløb for at kunne bruges.";
    pub const NO_SHEETS: &str = "Du har ingen afregninger endnu.";
    pub const DOWNLOAD_PDF: &str = "Hent PDF";
    pub const VOID_SHEET: &str = "Annullér afregning";
    pub const CONFIRM_VOID: &str =
        "Annullér afregningen? Bilagene bliver frigivet og kan bruges igen.";
    pub const SHEET_VOIDED_NOTICE: &str =
        "Afregningen er annulleret, og bilagene er frigivet. PDF'en er gemt til dokumentation.";
    pub const SHEET_NOT_FOUND: &str = "Afregningen findes ikke.";
    pub const ITEMS: &str = "bilag";
    pub const CREATED: &str = "Oprettet";
    pub const ERR_SHEET_EMPTY: &str = "Vælg mindst ét bilag.";
    pub const ERR_SHEET_STALE: &str =
        "Et eller flere bilag kan ikke længere bruges. Genindlæs siden, og prøv igen.";
    pub const ERR_SHEET_NOT_ACTIVE: &str = "Afregningen er allerede annulleret.";
    pub const ERR_TITLE_TOO_LONG: &str = "Titlen er for lang (maks. 120 tegn).";
    pub const BANK_ACCOUNT: &str = "Kontonummer";
    pub const BANK_ACCOUNT_HINT: &str =
        "Reg.nr. og kontonr. (eller IBAN) til udbetaling. Huskes til næste afregning.";
    pub const BANK_ACCOUNT_PLACEHOLDER: &str = "fx 1234 0001234567";
    pub const ERR_BANK_ACCOUNT: &str =
        "Ugyldigt kontonummer. Skriv reg.nr. og kontonr., fx 1234 0001234567, eller et IBAN.";

    pub const PDF_NAME: &str = "Navn";
    pub const PDF_SHEET_NO: &str = "Afregning nr.";
    pub const PDF_COUNT: &str = "Antal bilag";
    pub const PDF_FX_NOTE: &str =
        "Udenlandske beløb er omregnet til DKK med ECB's referencekurs på bilagsdatoen.";
    pub const PDF_VOIDED: &str = "ANNULLERET";
    pub const COL_NO: &str = "Nr.";
    pub const COL_DATE: &str = "Dato";
    pub const COL_CATEGORY: &str = "Udgiftstype";
    pub const COL_VENDOR: &str = "Forretning / beskrivelse";
    pub const COL_ORIGINAL: &str = "Originalbeløb";
    pub const COL_BASE: &str = "Beløb (DKK)";
    pub const BY_CATEGORY: &str = "Fordelt på udgiftstype";
    pub const TOTAL: &str = "I alt";
    pub const SIGN_EMPLOYEE: &str = "Medarbejder (dato og underskrift)";
    pub const SIGN_APPROVER: &str = "Godkendt af (dato og underskrift)";
    pub const PAGE: &str = "side";
    pub const OF: &str = "af";
    pub const PDF_HEIC_PLACEHOLDER: &str =
        "Billedet er i HEIC-format og kan ikke vises i PDF'en. Originalen er gemt i systemet.";
    pub const PDF_BROKEN_PLACEHOLDER: &str =
        "Filen kunne ikke indlejres i PDF'en. Originalen er gemt i systemet.";

    pub const ERR_FORBIDDEN: &str = "Du har ikke adgang til denne side.";
    pub const ADMIN_TITLE: &str = "Administration";
    pub const ADMIN_READ_ONLY: &str = "Du ser alle brugeres data. Visningen er skrivebeskyttet, og åbnede bilag og afregninger bliver logget.";
    pub const ADMIN_TAB_EXPENSES: &str = "Bilag";
    pub const ADMIN_TAB_SHEETS: &str = "Afregninger";
    pub const ADMIN_TAB_USERS: &str = "Brugere";
    pub const ADMIN_TAB_LOG: &str = "Log";
    pub const ALL_USERS: &str = "Alle brugere";
    pub const OWNER: &str = "Ejer";
    pub const DELETED: &str = "Slettet";
    pub const PROVIDER_ENTRA: &str = "Microsoft";
    pub const PROVIDER_LOCAL: &str = "Lokal";
    pub const LAST_LOGIN: &str = "Seneste login";
    pub const NEVER: &str = "aldrig";
    pub const USER_DISABLED: &str = "Deaktiveret";
    pub const SHOW_OLDER: &str = "Vis ældre";
    pub const NO_LOG_ENTRIES: &str = "Ingen hændelser.";
    pub const NO_USERS: &str = "Ingen brugere.";
    pub const SYSTEM: &str = "System";
    pub const NOT_SET: &str = "–";
    pub const LOG_FOR_USER: &str = "Log";
}

/// Danish label for an audit log action; unknown actions are shown as-is.
pub fn audit_action_label(action: &str) -> &str {
    match action {
        "login" => "Logget ind",
        "login_failed" => "Mislykket login",
        "login_denied_disabled" => "Login afvist (deaktiveret)",
        "logout" => "Logget ud",
        "user_created" => "Bruger oprettet",
        "user_bank_account_changed" => "Kontonummer ændret",
        "expense_created" => "Bilag oprettet",
        "attachment_added" => "Side tilføjet",
        "expense_updated" => "Bilag ændret",
        "expense_status_changed" => "Status ændret",
        "expense_deleted" => "Bilag slettet",
        "sheet_created" => "Afregning oprettet",
        "sheet_voided" => "Afregning annulleret",
        "sheet_pdf_stored" => "PDF gemt",
        "sheet_pdf_downloaded" => "PDF hentet",
        "admin_viewed_expense" => "Administrator åbnede bilag",
        "admin_viewed_sheet" => "Administrator åbnede afregning",
        other => other,
    }
}

/// Number of decimals used for amounts in the given ISO 4217 currency.
pub fn minor_units(currency: &str) -> u32 {
    match currency {
        "JPY" | "KRW" | "ISK" | "CLP" | "VND" => 0,
        _ => 2,
    }
}

fn group_thousands(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Formats an amount in minor units the Danish way, e.g. `1.234,56 kr.` or `1.234,56 EUR`.
pub fn format_amount(minor: i64, currency: &str) -> String {
    let number = format_number(minor, currency);
    if currency == "DKK" {
        format!("{number} kr.")
    } else {
        format!("{number} {currency}")
    }
}

/// Formats minor units as a Danish number without currency, e.g. `1.234,56`.
pub fn format_number(minor: i64, currency: &str) -> String {
    let decimals = minor_units(currency);
    let divisor = 10i64.pow(decimals);
    let abs = minor.unsigned_abs();
    let whole = group_thousands(&(abs / divisor as u64).to_string());
    let sign = if minor < 0 { "-" } else { "" };
    if decimals == 0 {
        format!("{sign}{whole}")
    } else {
        let frac = abs % divisor as u64;
        format!("{sign}{whole},{frac:0width$}", width = decimals as usize)
    }
}

/// Formats a stored rate like `7.460400` as `7,4604` (at least two decimals).
pub fn format_rate(rate: &str) -> String {
    let (whole, frac) = rate.split_once('.').unwrap_or((rate, ""));
    let mut frac = frac.trim_end_matches('0').to_string();
    while frac.len() < 2 {
        frac.push('0');
    }
    format!("{whole},{frac}")
}

pub fn format_date(date: NaiveDate) -> String {
    date.format("%d.%m.%Y").to_string()
}

fn last_sunday(year: i32, month: u32) -> NaiveDate {
    let first_of_next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .expect("valid date");
    let last = first_of_next - Days::new(1);
    last - Days::new(last.weekday().num_days_from_sunday() as u64)
}

/// Danish local time (CET/CEST, EU summer-time rule); identical on server and in the browser.
pub fn copenhagen_time(t: DateTime<Utc>) -> NaiveDateTime {
    let utc = t.naive_utc();
    let year = utc.year();
    let dst_start = last_sunday(year, 3)
        .and_hms_opt(1, 0, 0)
        .expect("valid time");
    let dst_end = last_sunday(year, 10)
        .and_hms_opt(1, 0, 0)
        .expect("valid time");
    let offset = if utc >= dst_start && utc < dst_end {
        2
    } else {
        1
    };
    utc + Duration::hours(offset)
}

/// E.g. `30.09.2026 kl. 14.05` in Danish time.
pub fn format_datetime(t: DateTime<Utc>) -> String {
    copenhagen_time(t).format("%d.%m.%Y kl. %H.%M").to_string()
}

/// Parses a user-entered positive amount (`1.234,56`, `1234,5`, `12.50`, `1 234`) into minor units.
pub fn parse_amount(input: &str, currency: &str) -> Option<i64> {
    let decimals = minor_units(currency) as usize;
    let cleaned: String = input
        .trim()
        .trim_end_matches("kr.")
        .trim_end_matches("kr")
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{a0}')
        .collect();
    if cleaned.is_empty()
        || !cleaned
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
    {
        return None;
    }

    let (whole, frac) = if let Some((w, f)) = cleaned.rsplit_once(',') {
        if w.contains(',') || f.contains('.') {
            return None;
        }
        (thousands_part(w)?, f.to_string())
    } else if let Some((w, f)) = cleaned.rsplit_once('.') {
        // A single dot followed by exactly three digits is a Danish thousands separator.
        if f.len() == 3 || w.contains('.') {
            (thousands_part(&cleaned)?, String::new())
        } else {
            (w.to_string(), f.to_string())
        }
    } else {
        (cleaned, String::new())
    };

    if whole.is_empty() || frac.len() > decimals {
        return None;
    }
    let whole: i64 = whole.parse().ok()?;
    let frac: i64 = if frac.is_empty() {
        0
    } else {
        format!("{frac:0<decimals$}").parse().ok()?
    };
    whole
        .checked_mul(10i64.pow(decimals as u32))?
        .checked_add(frac)
}

fn thousands_part(s: &str) -> Option<String> {
    let groups: Vec<&str> = s.split('.').collect();
    if groups.len() > 1
        && (groups[0].is_empty() || groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3))
    {
        return None;
    }
    Some(groups.concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_dkk() {
        assert_eq!(format_amount(123456, "DKK"), "1.234,56 kr.");
        assert_eq!(format_amount(5, "DKK"), "0,05 kr.");
        assert_eq!(format_amount(100000000, "DKK"), "1.000.000,00 kr.");
        assert_eq!(format_amount(-1050, "DKK"), "-10,50 kr.");
    }

    #[test]
    fn formats_foreign() {
        assert_eq!(format_amount(1999, "EUR"), "19,99 EUR");
        assert_eq!(format_amount(12345, "JPY"), "12.345 JPY");
    }

    #[test]
    fn formats_date() {
        assert_eq!(
            format_date(NaiveDate::from_ymd_opt(2026, 9, 30).unwrap()),
            "30.09.2026"
        );
    }

    #[test]
    fn formats_datetime_in_danish_time() {
        let utc = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
        assert_eq!(
            format_datetime(utc("2026-09-30T12:05:00Z")),
            "30.09.2026 kl. 14.05"
        );
        assert_eq!(
            format_datetime(utc("2026-12-31T23:30:00Z")),
            "01.01.2027 kl. 00.30"
        );
        // 2026 summer time: 29 March 01:00 UTC to 25 October 01:00 UTC.
        assert_eq!(
            format_datetime(utc("2026-03-29T00:59:00Z")),
            "29.03.2026 kl. 01.59"
        );
        assert_eq!(
            format_datetime(utc("2026-03-29T01:00:00Z")),
            "29.03.2026 kl. 03.00"
        );
        assert_eq!(
            format_datetime(utc("2026-10-25T00:59:00Z")),
            "25.10.2026 kl. 02.59"
        );
        assert_eq!(
            format_datetime(utc("2026-10-25T01:00:00Z")),
            "25.10.2026 kl. 02.00"
        );
    }

    #[test]
    fn formats_numbers_and_rates() {
        assert_eq!(format_number(123456, "DKK"), "1.234,56");
        assert_eq!(
            parse_amount(&format_number(123456, "DKK"), "DKK"),
            Some(123456)
        );
        assert_eq!(format_rate("7.460400"), "7,4604");
        assert_eq!(format_rate("1.000000"), "1,00");
        assert_eq!(format_rate("0.046590"), "0,04659");
    }

    #[test]
    fn parses_amounts() {
        assert_eq!(parse_amount("1.234,56", "DKK"), Some(123456));
        assert_eq!(parse_amount("1234,5", "DKK"), Some(123450));
        assert_eq!(parse_amount("12.50", "DKK"), Some(1250));
        assert_eq!(parse_amount("1.234", "DKK"), Some(123400));
        assert_eq!(parse_amount("1 234 kr.", "DKK"), Some(123400));
        assert_eq!(parse_amount("99", "DKK"), Some(9900));
        assert_eq!(parse_amount("1.000.000", "DKK"), Some(100000000));
        assert_eq!(parse_amount("500", "JPY"), Some(500));
    }

    #[test]
    fn rejects_bad_amounts() {
        assert_eq!(parse_amount("", "DKK"), None);
        assert_eq!(parse_amount("-5", "DKK"), None);
        assert_eq!(parse_amount("1,234,5", "DKK"), None);
        assert_eq!(parse_amount("1,234", "DKK"), None);
        assert_eq!(parse_amount("12.5.0", "DKK"), None);
        assert_eq!(parse_amount("abc", "DKK"), None);
        assert_eq!(parse_amount("5,5", "JPY"), None);
    }
}
