//! ECB euro reference rates and conversion to the base currency (DKK).

use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{Months, NaiveDate, Utc};
use sqlx::SqlitePool;

use crate::{i18n::minor_units, model::BASE_CURRENCY};

const HIST_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-hist.xml";
const HIST_90D_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-hist-90d.xml";
const REFRESH_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// A rate older than this relative to the expense date is treated as missing (stale feed).
const MAX_RATE_AGE_DAYS: i64 = 7;
const HISTORY_YEARS: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decimal {
    mantissa: i128,
    scale: u32,
}

impl Decimal {
    pub const ONE: Decimal = Decimal {
        mantissa: 1,
        scale: 0,
    };

    pub fn parse(s: &str) -> Option<Self> {
        let (whole, frac) = s.split_once('.').unwrap_or((s, ""));
        if whole.is_empty()
            || !whole
                .chars()
                .chain(frac.chars())
                .all(|c| c.is_ascii_digit())
            || frac.len() > 12
        {
            return None;
        }
        let mantissa: i128 = format!("{whole}{frac}").parse().ok()?;
        (mantissa > 0).then_some(Decimal {
            mantissa,
            scale: frac.len() as u32,
        })
    }
}

fn div_round(num: i128, den: i128) -> i128 {
    (2 * num + den) / (2 * den)
}

pub struct Converted {
    pub amount_base_minor: i64,
    /// DKK per one unit of the source currency, 6 decimals.
    pub rate: String,
}

/// Converts via EUR cross rates: both rates are "units per 1 EUR" as published by the ECB.
pub fn convert(
    amount_minor: i64,
    currency: &str,
    cur_per_eur: Decimal,
    base_per_eur: Decimal,
) -> Option<Converted> {
    let cur_dec = minor_units(currency);
    let base_dec = minor_units(BASE_CURRENCY);
    let p = |e: u32| 10i128.checked_pow(e);

    let num = (amount_minor as i128)
        .checked_mul(p(base_dec + cur_per_eur.scale)?)?
        .checked_mul(base_per_eur.mantissa)?;
    let den = p(cur_dec + base_per_eur.scale)?.checked_mul(cur_per_eur.mantissa)?;
    let amount_base_minor = i64::try_from(div_round(num, den)).ok()?;

    let rate_num = base_per_eur
        .mantissa
        .checked_mul(p(cur_per_eur.scale + 6)?)?;
    let rate_den = cur_per_eur.mantissa.checked_mul(p(base_per_eur.scale)?)?;
    let r = div_round(rate_num, rate_den);
    Some(Converted {
        amount_base_minor,
        rate: format!("{}.{:06}", r / 1_000_000, r % 1_000_000),
    })
}

pub struct FxResult {
    pub amount_base_minor: i64,
    pub rate: String,
    pub rate_date: NaiveDate,
}

async fn rate_on_or_before(
    pool: &SqlitePool,
    currency: &str,
    date: NaiveDate,
) -> sqlx::Result<Option<(NaiveDate, String)>> {
    sqlx::query_as(
        "SELECT rate_date, rate_per_eur FROM fx_rates
         WHERE currency = ? AND rate_date <= ? ORDER BY rate_date DESC LIMIT 1",
    )
    .bind(currency)
    .bind(date)
    .fetch_optional(pool)
    .await
}

/// Converts to DKK using the latest ECB rate published on or before `date`.
pub async fn to_base(
    pool: &SqlitePool,
    amount_minor: i64,
    currency: &str,
    date: NaiveDate,
) -> Result<Option<FxResult>> {
    if currency == BASE_CURRENCY {
        return Ok(Some(FxResult {
            amount_base_minor: amount_minor,
            rate: "1.000000".into(),
            rate_date: date,
        }));
    }
    let Some((rate_date, base_rate)) = rate_on_or_before(pool, BASE_CURRENCY, date).await? else {
        return Ok(None);
    };
    if (date - rate_date).num_days() > MAX_RATE_AGE_DAYS {
        return Ok(None);
    }
    let cur_rate = if currency == "EUR" {
        Some(Decimal::ONE)
    } else {
        rate_on_or_before(pool, currency, date)
            .await?
            .filter(|(d, _)| *d == rate_date)
            .and_then(|(_, r)| Decimal::parse(&r))
    };
    let (Some(cur_rate), Some(base_rate)) = (cur_rate, Decimal::parse(&base_rate)) else {
        return Ok(None);
    };
    Ok(
        convert(amount_minor, currency, cur_rate, base_rate).map(|c| FxResult {
            amount_base_minor: c.amount_base_minor,
            rate: c.rate,
            rate_date,
        }),
    )
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    ['\'', '"'].into_iter().find_map(|q| {
        let pat = format!("{name}={q}");
        let rest = &tag[tag.find(&pat)? + pat.len()..];
        rest.find(q).map(|end| &rest[..end])
    })
}

/// Parses the ECB `eurofxref` XML into (date, currency, rate-per-EUR) rows.
pub fn parse_ecb(xml: &str) -> Vec<(NaiveDate, String, String)> {
    let mut rows = Vec::new();
    let mut date = None;
    for tag in xml.split('<').filter(|t| t.starts_with("Cube ")) {
        if let Some(t) = attr(tag, "time") {
            date = NaiveDate::parse_from_str(t, "%Y-%m-%d").ok();
        } else if let (Some(d), Some(cur), Some(rate)) =
            (date, attr(tag, "currency"), attr(tag, "rate"))
        {
            if cur.len() == 3
                && cur.chars().all(|c| c.is_ascii_uppercase())
                && Decimal::parse(rate).is_some()
            {
                rows.push((d, cur.to_string(), rate.to_string()));
            }
        }
    }
    rows
}

pub async fn refresh(pool: &SqlitePool, http: &reqwest::Client) -> Result<usize> {
    let empty: bool = sqlx::query_scalar("SELECT NOT EXISTS (SELECT 1 FROM fx_rates)")
        .fetch_one(pool)
        .await?;
    let url = if empty { HIST_URL } else { HIST_90D_URL };
    let xml = http
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let cutoff = Utc::now().date_naive() - Months::new(12 * HISTORY_YEARS);

    let mut tx = pool.begin().await?;
    let mut inserted = 0;
    for (date, currency, rate) in parse_ecb(&xml).into_iter().filter(|(d, _, _)| *d >= cutoff) {
        inserted += sqlx::query(
            "INSERT OR IGNORE INTO fx_rates (rate_date, currency, rate_per_eur) VALUES (?, ?, ?)",
        )
        .bind(date)
        .bind(currency)
        .bind(rate)
        .execute(&mut *tx)
        .await?
        .rows_affected() as usize;
    }
    tx.commit().await?;
    Ok(inserted)
}

pub fn spawn_updater(pool: SqlitePool) -> Result<()> {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("building HTTP client")?;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(REFRESH_EVERY);
        loop {
            interval.tick().await;
            match refresh(&pool, &http).await {
                Ok(n) => tracing::info!("ECB rates refreshed, {n} new rows"),
                Err(e) => tracing::error!("ECB rate refresh failed: {e:#}"),
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).unwrap()
    }

    #[test]
    fn parses_decimals() {
        assert_eq!(
            d("7.4604"),
            Decimal {
                mantissa: 74604,
                scale: 4
            }
        );
        assert_eq!(
            d("160"),
            Decimal {
                mantissa: 160,
                scale: 0
            }
        );
        assert!(Decimal::parse("").is_none());
        assert!(Decimal::parse("0").is_none());
        assert!(Decimal::parse("1,5").is_none());
        assert!(Decimal::parse("-1").is_none());
    }

    #[test]
    fn converts_eur() {
        let c = convert(10000, "EUR", Decimal::ONE, d("7.4604")).unwrap();
        assert_eq!(c.amount_base_minor, 74604);
        assert_eq!(c.rate, "7.460400");
    }

    #[test]
    fn converts_cross_rate_and_minor_units() {
        // 1000 JPY at 160.12 JPY/EUR and 7.46 DKK/EUR = 46.5900... DKK
        let c = convert(1000, "JPY", d("160.12"), d("7.46")).unwrap();
        assert_eq!(c.amount_base_minor, 4659);
        assert_eq!(c.rate, "0.046590");

        // 123.45 SEK at 11.2 SEK/EUR and 7.4604 DKK/EUR = 82.2310... DKK
        let c = convert(12345, "SEK", d("11.2"), d("7.4604")).unwrap();
        assert_eq!(c.amount_base_minor, 8223);
    }

    #[test]
    fn parses_ecb_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<gesmes:Envelope><Cube>
<Cube time='2026-09-29'><Cube currency='USD' rate='1.1745'/><Cube currency='DKK' rate='7.4604'/></Cube>
<Cube time="2026-09-28"><Cube currency="JPY" rate="160.12"/><Cube currency='bad' rate='1'/></Cube>
</Cube></gesmes:Envelope>"#;
        let rows = parse_ecb(xml);
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[0],
            (
                NaiveDate::from_ymd_opt(2026, 9, 29).unwrap(),
                "USD".into(),
                "1.1745".into()
            )
        );
        assert_eq!(rows[2].1, "JPY");
    }
}
