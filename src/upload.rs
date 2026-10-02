//! Browser-side upload: shrinks photos before sending them. Only runs in the browser.

use wasm_bindgen::{JsCast, JsValue, prelude::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, CanvasRenderingContext2d, File, FormData, HtmlCanvasElement, ImageBitmap, RequestInit,
    Response,
};

use crate::{i18n::t, model::UploadResponse};

const MAX_DIMENSION: f64 = 2400.0;
const JPEG_QUALITY: f64 = 0.85;
const SHRINK_ABOVE_BYTES: f64 = 1_500_000.0;

fn failed(_: JsValue) -> String {
    t::UPLOAD_FAILED.to_string()
}

fn jpeg_name(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    format!("{stem}.jpg")
}

/// Re-encodes large JPEG/PNG/WebP photos as a smaller JPEG; `None` means upload the original.
async fn shrink_image(file: &File) -> Option<Blob> {
    if !matches!(
        file.type_().as_str(),
        "image/jpeg" | "image/png" | "image/webp"
    ) {
        return None;
    }
    let window = web_sys::window()?;
    // createImageBitmap applies EXIF orientation by default, so rotated phone photos come out upright.
    let bitmap: ImageBitmap = JsFuture::from(window.create_image_bitmap_with_blob(file).ok()?)
        .await
        .ok()?
        .dyn_into()
        .ok()?;
    let (w, h) = (bitmap.width() as f64, bitmap.height() as f64);
    let scale = (MAX_DIMENSION / w.max(h)).min(1.0);
    if scale >= 1.0 && file.size() < SHRINK_ABOVE_BYTES {
        return None;
    }
    let (nw, nh) = ((w * scale).round(), (h * scale).round());

    let canvas: HtmlCanvasElement = window
        .document()?
        .create_element("canvas")
        .ok()?
        .dyn_into()
        .ok()?;
    canvas.set_width(nw as u32);
    canvas.set_height(nh as u32);
    let ctx: CanvasRenderingContext2d = canvas.get_context("2d").ok()??.dyn_into().ok()?;
    // JPEG has no alpha channel; paint transparent PNG areas white instead of black.
    ctx.set_fill_style_str("#fff");
    ctx.fill_rect(0.0, 0.0, nw, nh);
    ctx.draw_image_with_image_bitmap_and_dw_and_dh(&bitmap, 0.0, 0.0, nw, nh)
        .ok()?;
    bitmap.close();

    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let callback = Closure::once_into_js(move |blob: JsValue| {
            let _ = resolve.call1(&JsValue::NULL, &blob);
        });
        let _ = canvas.to_blob_with_type_and_encoder_options(
            callback.unchecked_ref(),
            "image/jpeg",
            &JsValue::from_f64(JPEG_QUALITY),
        );
    });
    JsFuture::from(promise).await.ok()?.dyn_into::<Blob>().ok()
}

/// Uploads files as a new voucher, or as extra pages when `expense_id` is given. Returns the voucher id.
pub async fn upload(
    files: Vec<File>,
    expense_id: Option<i64>,
    autocrop: bool,
) -> Result<i64, String> {
    let form = FormData::new().map_err(failed)?;
    if let Some(id) = expense_id {
        form.append_with_str("expense_id", &id.to_string())
            .map_err(failed)?;
    }
    form.append_with_str("autocrop", if autocrop { "1" } else { "0" })
        .map_err(failed)?;
    for file in &files {
        match shrink_image(file).await {
            Some(blob) => {
                form.append_with_blob_and_filename("files", &blob, &jpeg_name(&file.name()))
            }
            None => form.append_with_blob_and_filename("files", file, &file.name()),
        }
        .map_err(failed)?;
    }

    let init = RequestInit::new();
    init.set_method("POST");
    init.set_body(&form);
    let window = web_sys::window().ok_or_else(|| t::UPLOAD_FAILED.to_string())?;
    let response: Response = JsFuture::from(window.fetch_with_str_and_init("/bilag/upload", &init))
        .await
        .map_err(failed)?
        .dyn_into()
        .map_err(failed)?;
    let text = JsFuture::from(response.text().map_err(failed)?)
        .await
        .map_err(failed)?
        .as_string()
        .unwrap_or_default();

    match response.status() {
        200 => serde_json::from_str::<UploadResponse>(&text)
            .map(|r| r.expense_id)
            .map_err(|_| t::UPLOAD_FAILED.to_string()),
        413 => Err(t::ERR_FILE_TOO_LARGE.to_string()),
        400 if !text.is_empty() => Err(text),
        _ => Err(t::UPLOAD_FAILED.to_string()),
    }
}
