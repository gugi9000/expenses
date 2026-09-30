use std::{future::Future, pin::Pin};

use chrono::NaiveDate;

/// Fields an OCR service could pre-fill; the user always confirms them.
#[derive(Debug, Default)]
pub struct OcrSuggestion {
    pub vendor: Option<String>,
    pub expense_date: Option<NaiveDate>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
}

pub type OcrFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<Option<OcrSuggestion>>> + Send + 'a>>;

/// Hook for a future OCR backend (e.g. Azure AI Document Intelligence).
pub trait OcrProvider: Send + Sync {
    fn extract<'a>(&'a self, mime: &'a str, bytes: &'a [u8]) -> OcrFuture<'a>;
}

pub struct NoopOcr;

impl OcrProvider for NoopOcr {
    fn extract<'a>(&'a self, _mime: &'a str, _bytes: &'a [u8]) -> OcrFuture<'a> {
        Box::pin(async { Ok(None) })
    }
}
