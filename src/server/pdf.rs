//! Renders an expense sheet PDF: summary table, then every voucher page stamped with its number.

use std::io::Cursor;

use anyhow::{Context, Result, bail};
use image::{
    ImageDecoder, RgbImage,
    codecs::jpeg::{JpegDecoder, JpegEncoder},
    metadata::Orientation,
};
use lopdf::{
    Dictionary, Document, Object, ObjectId, Stream, StringFormat,
    content::{Content, Operation},
    dictionary,
};

use crate::{
    i18n::{format_amount, format_date, format_datetime, t},
    model::{BASE_CURRENCY, SheetItem, SheetStatus, SheetSummary, totals_by_category},
};

pub struct PdfAttachment {
    pub mime: String,
    pub bytes: Vec<u8>,
}

pub struct PdfItem {
    pub item: SheetItem,
    pub attachments: Vec<PdfAttachment>,
}

pub struct PdfSheet {
    pub summary: SheetSummary,
    pub owner_name: String,
    pub bank_account: Option<String>,
    pub items: Vec<PdfItem>,
}

const PAGE_W: f32 = 595.28;
const PAGE_H: f32 = 841.89;
const MARGIN: f32 = 42.0;
const ROW_H: f32 = 15.0;
const X_NO: f32 = MARGIN;
const X_DATE: f32 = MARGIN + 26.0;
const X_CATEGORY: f32 = MARGIN + 86.0;
const X_VENDOR: f32 = MARGIN + 178.0;
const X_ORIGINAL_RIGHT: f32 = PAGE_W - MARGIN - 86.0;
const X_BASE_RIGHT: f32 = PAGE_W - MARGIN;
const STAMP_FONT: &str = "FUdgStamp";

#[derive(Clone, Copy)]
enum Font {
    Regular,
    Bold,
}

impl Font {
    fn resource_name(self) -> &'static str {
        match self {
            Font::Regular => "F1",
            Font::Bold => "F2",
        }
    }
}

/// Glyph widths (1/1000 em) of Helvetica and Helvetica-Bold for ASCII 32..=126.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];
const HELVETICA_BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667,
    611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556,
    278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

fn char_width(c: char, font: Font) -> u16 {
    let table = match font {
        Font::Regular => &HELVETICA,
        Font::Bold => &HELVETICA_BOLD,
    };
    match c {
        ' '..='~' => table[c as usize - 32],
        '…' | 'Æ' => 1000,
        'æ' => 889,
        'Ø' | 'Ö' => 778,
        'Å' | 'Ä' => 667,
        'ø' => 611,
        _ => 556,
    }
}

fn text_width(s: &str, font: Font, size: f32) -> f32 {
    s.chars().map(|c| char_width(c, font) as f32).sum::<f32>() * size / 1000.0
}

/// Standard PDF fonts use WinAnsiEncoding (Windows-1252), which covers Danish letters.
fn winansi(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c {
            ' '..='~' => c as u8,
            '\u{a0}'..='\u{ff}' => c as u32 as u8,
            '€' => 0x80,
            '…' => 0x85,
            '–' => 0x96,
            '—' => 0x97,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '•' => 0x95,
            _ => b'?',
        })
        .collect()
}

/// Collapses whitespace and cuts the text with an ellipsis so it fits `max_width`.
fn fit(s: &str, font: Font, size: f32, max_width: f32) -> String {
    let clean = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if text_width(&clean, font, size) <= max_width {
        return clean;
    }
    let budget = max_width - text_width("…", font, size);
    let mut out = String::new();
    let mut width = 0.0;
    for c in clean.chars() {
        width += char_width(c, font) as f32 * size / 1000.0;
        if width > budget {
            break;
        }
        out.push(c);
    }
    format!("{}…", out.trim_end())
}

#[derive(Default)]
struct Canvas {
    ops: Vec<Operation>,
}

impl Canvas {
    fn gray(&mut self, level: f32) {
        self.ops.push(Operation::new("g", vec![level.into()]));
    }

    fn text_with(&mut self, font_name: &str, size: f32, x: f32, y: f32, s: &str) {
        self.ops.push(Operation::new("BT", vec![]));
        self.ops
            .push(Operation::new("Tf", vec![font_name.into(), size.into()]));
        self.ops
            .push(Operation::new("Td", vec![x.into(), y.into()]));
        self.ops.push(Operation::new(
            "Tj",
            vec![Object::String(winansi(s), StringFormat::Literal)],
        ));
        self.ops.push(Operation::new("ET", vec![]));
    }

    fn text(&mut self, font: Font, size: f32, x: f32, y: f32, s: &str) {
        self.text_with(font.resource_name(), size, x, y, s);
    }

    fn text_right(&mut self, font: Font, size: f32, right: f32, y: f32, s: &str) {
        self.text(font, size, right - text_width(s, font, size), y, s);
    }

    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, gray: f32) {
        self.ops.push(Operation::new("G", vec![gray.into()]));
        self.ops.push(Operation::new("w", vec![width.into()]));
        self.ops
            .push(Operation::new("m", vec![x1.into(), y1.into()]));
        self.ops
            .push(Operation::new("l", vec![x2.into(), y2.into()]));
        self.ops.push(Operation::new("S", vec![]));
    }

    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, gray: f32) {
        self.gray(gray);
        self.ops.push(Operation::new(
            "re",
            vec![x.into(), y.into(), w.into(), h.into()],
        ));
        self.ops.push(Operation::new("f", vec![]));
        self.gray(0.0);
    }

    fn image(&mut self, name: &str, x: f32, y: f32, w: f32, h: f32) {
        self.ops.push(Operation::new("q", vec![]));
        self.ops.push(Operation::new(
            "cm",
            vec![w.into(), 0.into(), 0.into(), h.into(), x.into(), y.into()],
        ));
        self.ops.push(Operation::new("Do", vec![name.into()]));
        self.ops.push(Operation::new("Q", vec![]));
    }

    fn encode(self) -> Result<Vec<u8>> {
        Ok(Content {
            operations: self.ops,
        }
        .encode()?)
    }
}

struct PdfWriter {
    doc: Document,
    pages_id: ObjectId,
    regular: ObjectId,
    bold: ObjectId,
    kids: Vec<Object>,
}

fn a4() -> Object {
    vec![0.into(), 0.into(), PAGE_W.into(), PAGE_H.into()].into()
}

fn utf16_text(s: &str) -> Object {
    let mut bytes = vec![0xFE, 0xFF];
    bytes.extend(s.encode_utf16().flat_map(u16::to_be_bytes));
    Object::String(bytes, StringFormat::Hexadecimal)
}

impl PdfWriter {
    fn new() -> Self {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let font = |name: &str| {
            dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => name,
                "Encoding" => "WinAnsiEncoding",
            }
        };
        let regular = doc.add_object(font("Helvetica"));
        let bold = doc.add_object(font("Helvetica-Bold"));
        Self {
            doc,
            pages_id,
            regular,
            bold,
            kids: Vec::new(),
        }
    }

    fn add_page(&mut self, canvas: Canvas, xobjects: Option<Dictionary>) -> Result<()> {
        let mut content = Stream::new(Dictionary::new(), canvas.encode()?);
        let _ = content.compress();
        let content_id = self.doc.add_object(content);
        let mut resources = dictionary! {
            "Font" => dictionary! { "F1" => self.regular, "F2" => self.bold },
        };
        if let Some(x) = xobjects {
            resources.set("XObject", x);
        }
        let page = self.doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => self.pages_id,
            "MediaBox" => a4(),
            "Contents" => content_id,
            "Resources" => resources,
        });
        self.kids.push(page.into());
        Ok(())
    }

    fn add_image_page(&mut self, mime: &str, bytes: &[u8], header: &str) -> Result<()> {
        let (data, w, h, color_space) = jpeg_for_pdf(mime, bytes)?;
        let image = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => w as i64,
                "Height" => h as i64,
                "ColorSpace" => color_space,
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            data,
        )
        .with_compression(false);
        let image_id = self.doc.add_object(image);

        let mut c = Canvas::default();
        c.text(
            Font::Bold,
            10.0,
            MARGIN,
            PAGE_H - MARGIN - 10.0,
            &fit(header, Font::Bold, 10.0, PAGE_W - 2.0 * MARGIN),
        );
        let (box_w, box_h) = (PAGE_W - 2.0 * MARGIN, PAGE_H - 2.0 * MARGIN - 24.0);
        let scale = (box_w / w as f32).min(box_h / h as f32);
        let (dw, dh) = (w as f32 * scale, h as f32 * scale);
        let x = MARGIN + (box_w - dw) / 2.0;
        let y = MARGIN + (box_h - dh);
        c.image("Im0", x, y, dw, dh);
        self.add_page(c, Some(dictionary! { "Im0" => image_id }))
    }

    fn add_placeholder_page(&mut self, header: &str, message: &str) -> Result<()> {
        let mut c = Canvas::default();
        c.text(
            Font::Bold,
            10.0,
            MARGIN,
            PAGE_H - MARGIN - 10.0,
            &fit(header, Font::Bold, 10.0, PAGE_W - 2.0 * MARGIN),
        );
        c.text(
            Font::Regular,
            11.0,
            MARGIN,
            PAGE_H / 2.0,
            &fit(message, Font::Regular, 11.0, PAGE_W - 2.0 * MARGIN),
        );
        self.add_page(c, None)
    }

    /// Copies all pages of another PDF and stamps each with `stamp`.
    fn append_pdf(&mut self, bytes: &[u8], stamp: &str) -> Result<()> {
        let mut src = Document::load_mem(bytes).context("reading attached PDF")?;
        if src.is_encrypted() {
            src.decrypt("")
                .context("attached PDF is password protected")?;
        }
        let page_ids: Vec<ObjectId> = src.get_pages().into_values().collect();
        if page_ids.is_empty() {
            bail!("attached PDF has no pages");
        }
        // Pages are moved under our own page tree, so inherited attributes must be copied onto them first.
        for &id in &page_ids {
            inherit_page_attributes(&mut src, id)?;
        }
        src.renumber_objects_with(self.doc.max_id + 1);
        let page_ids: Vec<ObjectId> = src.get_pages().into_values().collect();
        let src_max = src.objects.keys().map(|(n, _)| *n).max().unwrap_or(0);
        self.doc.max_id = self.doc.max_id.max(src_max);

        for (id, object) in src.objects {
            let skip = object
                .type_name()
                .is_ok_and(|t| matches!(t, b"Catalog" | b"Pages" | b"Outlines" | b"Outline"));
            if !skip {
                self.doc.objects.insert(id, object);
            }
        }
        for id in page_ids {
            self.stamp_foreign_page(id, stamp)?;
            self.kids.push(id.into());
        }
        Ok(())
    }

    fn resolve_dict(&self, object: Option<&Object>) -> Dictionary {
        match object {
            Some(Object::Reference(r)) => self
                .doc
                .get_dictionary(*r)
                .cloned()
                .unwrap_or_else(|_| Dictionary::new()),
            Some(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        }
    }

    fn stamp_foreign_page(&mut self, page_id: ObjectId, stamp: &str) -> Result<()> {
        let page = self.doc.get_dictionary(page_id)?.clone();
        let mut resources = self.resolve_dict(page.get(b"Resources").ok());
        let mut fonts = self.resolve_dict(resources.get(b"Font").ok());
        fonts.set(STAMP_FONT, self.bold);
        resources.set("Font", fonts);

        let media_box = match page.get(b"MediaBox") {
            Ok(Object::Reference(r)) => self.doc.get_object(*r).ok().cloned(),
            Ok(o) => Some(o.clone()),
            Err(_) => None,
        };
        let origin = |i: usize| {
            media_box
                .as_ref()
                .and_then(|m| m.as_array().ok())
                .and_then(|a| a.get(i))
                .and_then(|v| v.as_float().ok())
                .unwrap_or(0.0)
        };
        let (llx, lly) = (origin(0), origin(1));

        let mut contents: Vec<Object> = match page.get(b"Contents") {
            Ok(Object::Array(a)) => a.clone(),
            Ok(Object::Reference(r)) => match self.doc.get_object(*r) {
                Ok(Object::Array(a)) => a.clone(),
                _ => vec![Object::Reference(*r)],
            },
            _ => Vec::new(),
        };

        // Wrap the original content in q/Q so its graphics state can't displace the stamp.
        let open = self
            .doc
            .add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
        let mut c = Canvas::default();
        c.ops.push(Operation::new("Q", vec![]));
        let size = 9.0;
        let width = text_width(stamp, Font::Bold, size);
        c.fill_rect(llx + 12.0, lly + 10.0, width + 12.0, size + 8.0, 1.0);
        c.text_with(STAMP_FONT, size, llx + 18.0, lly + 15.0, stamp);
        let close = self
            .doc
            .add_object(Stream::new(Dictionary::new(), c.encode()?));
        contents.insert(0, open.into());
        contents.push(close.into());

        let page = self.doc.get_dictionary_mut(page_id)?;
        page.set("Contents", contents);
        page.set("Resources", resources);
        page.set("Parent", self.pages_id);
        Ok(())
    }

    fn finish(mut self, title: &str) -> Result<Vec<u8>> {
        let count = self.kids.len() as i64;
        self.doc.objects.insert(
            self.pages_id,
            dictionary! { "Type" => "Pages", "Kids" => self.kids, "Count" => count }.into(),
        );
        let catalog = self
            .doc
            .add_object(dictionary! { "Type" => "Catalog", "Pages" => self.pages_id });
        let info = self.doc.add_object(dictionary! {
            "Title" => utf16_text(title),
            "Producer" => utf16_text(t::APP_NAME),
        });
        self.doc.trailer.set("Root", catalog);
        self.doc.trailer.set("Info", info);
        let mut out = Vec::new();
        self.doc.save_to(&mut out)?;
        Ok(out)
    }
}

fn inherit_page_attributes(doc: &mut Document, page_id: ObjectId) -> Result<()> {
    const INHERITABLE: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
    let page = doc.get_dictionary(page_id)?;
    let mut missing: Vec<&[u8]> = INHERITABLE
        .into_iter()
        .filter(|k| page.get(k).is_err())
        .collect();
    let mut parent = page.get(b"Parent").and_then(Object::as_reference).ok();
    let mut found = Vec::new();
    for _ in 0..32 {
        let (Some(id), false) = (parent, missing.is_empty()) else {
            break;
        };
        let Ok(node) = doc.get_dictionary(id) else {
            break;
        };
        missing.retain(|k| match node.get(k) {
            Ok(v) => {
                found.push((k.to_vec(), v.clone()));
                false
            }
            Err(_) => true,
        });
        parent = node.get(b"Parent").and_then(Object::as_reference).ok();
    }
    let page = doc.get_dictionary_mut(page_id)?;
    for (k, v) in found {
        page.set(k, v);
    }
    if page.get(b"MediaBox").is_err() {
        page.set("MediaBox", a4());
    }
    Ok(())
}

/// Reads width, height and colour components from a JPEG's start-of-frame marker.
fn jpeg_frame(b: &[u8]) -> Option<(u32, u32, u8)> {
    if b.get(0..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut i = 2;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            return None;
        }
        let marker = b[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if matches!(marker, 0x01 | 0xD0..=0xD8) {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let f = b.get(i + 4..i + 10)?;
            let h = u16::from_be_bytes([f[1], f[2]]) as u32;
            let w = u16::from_be_bytes([f[3], f[4]]) as u32;
            return Some((w, h, f[5]));
        }
        i += 2 + len;
    }
    None
}

pub(crate) fn jpeg_orientation(bytes: &[u8]) -> Orientation {
    JpegDecoder::new(Cursor::new(bytes))
        .and_then(|mut d| d.orientation())
        .unwrap_or(Orientation::NoTransforms)
}

/// Returns JPEG data ready for a DCTDecode image: upright JPEGs pass through untouched, others are re-encoded.
fn jpeg_for_pdf(mime: &str, bytes: &[u8]) -> Result<(Vec<u8>, u32, u32, &'static str)> {
    let is_jpeg = mime == "image/jpeg";
    if is_jpeg
        && let Some((w, h, components)) = jpeg_frame(bytes)
        && jpeg_orientation(bytes) == Orientation::NoTransforms
    {
        match components {
            1 => return Ok((bytes.to_vec(), w, h, "DeviceGray")),
            3 => return Ok((bytes.to_vec(), w, h, "DeviceRGB")),
            _ => {}
        }
    }

    let mut img = image::load_from_memory(bytes).context("decoding image")?;
    if is_jpeg {
        img.apply_orientation(jpeg_orientation(bytes));
    }
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    // JPEG has no alpha: blend transparent areas onto white.
    let mut rgb = RgbImage::new(w, h);
    for (out, px) in rgb.pixels_mut().zip(rgba.pixels()) {
        let a = px[3] as u32;
        for i in 0..3 {
            out[i] = ((px[i] as u32 * a + 255 * (255 - a)) / 255) as u8;
        }
    }
    let mut data = Vec::new();
    JpegEncoder::new_with_quality(&mut data, 85).encode_image(&rgb)?;
    Ok((data, w, h, "DeviceRGB"))
}

fn table_header(c: &mut Canvas, y: f32) -> f32 {
    c.fill_rect(MARGIN, y - ROW_H, PAGE_W - 2.0 * MARGIN, ROW_H, 0.9);
    let base = y - 10.5;
    c.text(Font::Bold, 8.5, X_NO + 2.0, base, t::COL_NO);
    c.text(Font::Bold, 8.5, X_DATE, base, t::COL_DATE);
    c.text(Font::Bold, 8.5, X_CATEGORY, base, t::COL_CATEGORY);
    c.text(Font::Bold, 8.5, X_VENDOR, base, t::COL_VENDOR);
    c.text_right(Font::Bold, 8.5, X_ORIGINAL_RIGHT, base, t::COL_ORIGINAL);
    c.text_right(Font::Bold, 8.5, X_BASE_RIGHT - 2.0, base, t::COL_BASE);
    y - ROW_H
}

fn summary_pages(sheet: &PdfSheet) -> Vec<Canvas> {
    let s = &sheet.summary;
    let content_w = PAGE_W - 2.0 * MARGIN;
    let top = PAGE_H - MARGIN;
    let bottom = MARGIN + 30.0;
    let mut pages = Vec::new();
    let mut c = Canvas::default();
    let mut y = top;

    c.text(Font::Bold, 18.0, MARGIN, y - 18.0, t::EXPENSE_SHEET);
    if s.status == SheetStatus::Voided {
        c.gray(0.45);
        c.text_right(Font::Bold, 14.0, PAGE_W - MARGIN, y - 18.0, t::PDF_VOIDED);
        c.gray(0.0);
    }
    y -= 30.0;
    c.text(
        Font::Regular,
        12.0,
        MARGIN,
        y - 12.0,
        &fit(&s.title, Font::Regular, 12.0, content_w),
    );
    y -= 26.0;
    for (label, value) in [
        (t::PDF_NAME, Some(sheet.owner_name.clone())),
        (t::BANK_ACCOUNT, sheet.bank_account.clone()),
        (t::PDF_SHEET_NO, Some(s.id.to_string())),
        (t::CREATED, Some(format_datetime(s.created_at))),
        (t::PDF_COUNT, Some(sheet.items.len().to_string())),
    ]
    .into_iter()
    .filter_map(|(label, value)| Some((label, value?)))
    {
        c.text(Font::Bold, 9.5, MARGIN, y - 10.0, label);
        c.text(
            Font::Regular,
            9.5,
            MARGIN + 90.0,
            y - 10.0,
            &fit(&value, Font::Regular, 9.5, content_w - 90.0),
        );
        y -= 14.0;
    }
    c.gray(0.4);
    c.text(Font::Regular, 8.5, MARGIN, y - 10.0, t::PDF_FX_NOTE);
    c.gray(0.0);
    y -= 24.0;
    y = table_header(&mut c, y);

    for PdfItem { item, .. } in &sheet.items {
        if y - ROW_H < bottom {
            pages.push(std::mem::take(&mut c));
            y = table_header(&mut c, top);
        }
        let base = y - 10.5;
        let size = 9.0;
        c.text(
            Font::Regular,
            size,
            X_NO + 2.0,
            base,
            &item.position.to_string(),
        );
        c.text(
            Font::Regular,
            size,
            X_DATE,
            base,
            &format_date(item.expense_date),
        );
        let category = item.category.as_deref().unwrap_or("–");
        c.text(
            Font::Regular,
            size,
            X_CATEGORY,
            base,
            &fit(category, Font::Regular, size, X_VENDOR - X_CATEGORY - 6.0),
        );
        let vendor = [item.vendor.as_deref(), item.description.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" – ");
        let vendor = if vendor.is_empty() {
            item.kind.label().to_string()
        } else {
            vendor
        };
        c.text(
            Font::Regular,
            size,
            X_VENDOR,
            base,
            &fit(
                &vendor,
                Font::Regular,
                size,
                X_ORIGINAL_RIGHT - 80.0 - X_VENDOR,
            ),
        );
        if item.currency != BASE_CURRENCY {
            c.text_right(
                Font::Regular,
                size,
                X_ORIGINAL_RIGHT,
                base,
                &format_amount(item.amount_minor, &item.currency),
            );
        }
        c.text_right(
            Font::Regular,
            size,
            X_BASE_RIGHT - 2.0,
            base,
            &format_amount(item.amount_base_minor, BASE_CURRENCY),
        );
        y -= ROW_H;
        c.line(MARGIN, y, PAGE_W - MARGIN, y, 0.4, 0.8);
    }

    let categories = totals_by_category(
        sheet
            .items
            .iter()
            .map(|i| (i.item.category.as_deref(), i.item.amount_base_minor)),
    );
    let needed = (categories.len() as f32 + 3.0) * ROW_H + 100.0;
    if y - needed < bottom {
        pages.push(std::mem::take(&mut c));
        y = top;
    }
    y -= 14.0;
    c.text(Font::Bold, 10.0, MARGIN, y - 11.0, t::BY_CATEGORY);
    y -= ROW_H + 2.0;
    for (name, sum) in &categories {
        c.text(
            Font::Regular,
            9.0,
            MARGIN,
            y - 11.0,
            &fit(name, Font::Regular, 9.0, content_w - 120.0),
        );
        c.text_right(
            Font::Regular,
            9.0,
            X_BASE_RIGHT - 2.0,
            y - 11.0,
            &format_amount(*sum, BASE_CURRENCY),
        );
        y -= ROW_H;
    }
    y -= 4.0;
    c.line(MARGIN, y, PAGE_W - MARGIN, y, 1.0, 0.0);
    y -= 4.0;
    c.text(Font::Bold, 11.0, MARGIN, y - 12.0, t::TOTAL);
    c.text_right(
        Font::Bold,
        11.0,
        X_BASE_RIGHT - 2.0,
        y - 12.0,
        &format_amount(s.total_base_minor, BASE_CURRENCY),
    );
    y -= 70.0;

    let sign_w = (content_w - 30.0) / 2.0;
    for (i, label) in [t::SIGN_EMPLOYEE, t::SIGN_APPROVER].into_iter().enumerate() {
        let x = MARGIN + i as f32 * (sign_w + 30.0);
        c.line(x, y, x + sign_w, y, 0.6, 0.0);
        c.gray(0.4);
        c.text(Font::Regular, 8.0, x, y - 11.0, label);
        c.gray(0.0);
    }
    pages.push(c);

    let total = pages.len();
    for (i, page) in pages.iter_mut().enumerate() {
        page.gray(0.4);
        page.text(
            Font::Regular,
            8.0,
            MARGIN,
            24.0,
            &format!(
                "{} #{} – {} {} {} {}",
                t::EXPENSE_SHEET,
                s.id,
                t::PAGE,
                i + 1,
                t::OF,
                total
            ),
        );
        page.gray(0.0);
    }
    pages
}

pub fn render(sheet: &PdfSheet) -> Result<Vec<u8>> {
    let mut pdf = PdfWriter::new();
    for page in summary_pages(sheet) {
        pdf.add_page(page, None)?;
    }

    for PdfItem { item, attachments } in &sheet.items {
        let stamp = format!(
            "{} #{} · {} {}",
            t::EXPENSE_SHEET,
            sheet.summary.id,
            t::VOUCHER,
            item.position
        );
        let details = [
            Some(format_date(item.expense_date)),
            item.vendor.clone(),
            Some(format_amount(item.amount_minor, &item.currency)),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        let header = format!("{stamp} · {details}");

        for a in attachments {
            let result = match a.mime.as_str() {
                "application/pdf" => pdf.append_pdf(&a.bytes, &stamp),
                "image/heic" | "image/heif" => {
                    pdf.add_placeholder_page(&header, t::PDF_HEIC_PLACEHOLDER)
                }
                mime => pdf.add_image_page(mime, &a.bytes, &header),
            };
            if let Err(e) = result {
                tracing::warn!(
                    "sheet {}: voucher {} could not be embedded: {e:#}",
                    sheet.summary.id,
                    item.expense_id
                );
                pdf.add_placeholder_page(&header, t::PDF_BROKEN_PLACEHOLDER)?;
            }
        }
    }
    pdf.finish(&sheet.summary.title)
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, Utc};
    use image::{Rgba, RgbaImage};

    use super::*;
    use crate::model::ExpenseKind;

    fn item(position: usize, currency: &str, category: &str) -> SheetItem {
        SheetItem {
            position,
            expense_id: position as i64,
            kind: ExpenseKind::Receipt,
            category: Some(category.into()),
            vendor: Some(
                "Café Østergade (æøå) med et meget langt navn der skal forkortes i tabellen".into(),
            ),
            description: Some("Frokost\nmed kunde".into()),
            expense_date: NaiveDate::from_ymd_opt(2026, 9, 29).unwrap(),
            amount_minor: 12345,
            currency: currency.into(),
            fx_rate: "7.460400".into(),
            amount_base_minor: 92102,
        }
    }

    fn sheet(items: Vec<PdfItem>) -> PdfSheet {
        let total = items.iter().map(|i| i.item.amount_base_minor).sum();
        PdfSheet {
            summary: SheetSummary {
                id: 7,
                owner_name: "Bjarke Sørensen".into(),
                title: "September 2026".into(),
                status: SheetStatus::Active,
                created_at: Utc::now(),
                item_count: items.len() as i64,
                total_base_minor: total,
            },
            owner_name: "Bjarke Sørensen".into(),
            bank_account: Some("1234 0001234567".into()),
            items,
        }
    }

    fn encode_jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_pixel(w, h, image::Rgb([200, 30, 30]));
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, 80)
            .encode_image(&img)
            .unwrap();
        out
    }

    fn encode_png_with_alpha() -> Vec<u8> {
        let img = RgbaImage::from_pixel(40, 20, Rgba([0, 0, 255, 0]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn reads_jpeg_frame() {
        assert_eq!(jpeg_frame(&encode_jpeg(30, 20)), Some((30, 20, 3)));
        assert_eq!(jpeg_frame(b"not a jpeg"), None);
    }

    #[test]
    fn fits_and_encodes_text() {
        let s = fit("Østergade\n  Café", Font::Regular, 10.0, 1000.0);
        assert_eq!(s, "Østergade Café");
        let cut = fit(&"x".repeat(200), Font::Regular, 10.0, 50.0);
        assert!(cut.ends_with('…') && text_width(&cut, Font::Regular, 10.0) <= 50.0);
        assert_eq!(winansi("æøå€✓"), vec![0xE6, 0xF8, 0xE5, 0x80, b'?']);
    }

    #[test]
    fn renders_sheet_with_all_attachment_kinds() {
        // A previously rendered sheet doubles as a multi-page "uploaded PDF".
        let many: Vec<PdfItem> = (1..=60)
            .map(|i| PdfItem {
                item: item(i, "DKK", "Rejse"),
                attachments: vec![],
            })
            .collect();
        let attached_pdf = render(&sheet(many)).unwrap();
        let attached_pages = Document::load_mem(&attached_pdf).unwrap().get_pages().len();
        assert!(
            attached_pages >= 2,
            "60 rows should need more than one summary page"
        );

        let items = vec![
            PdfItem {
                item: item(1, "EUR", "Forplejning"),
                attachments: vec![
                    PdfAttachment {
                        mime: "image/jpeg".into(),
                        bytes: encode_jpeg(1200, 1600),
                    },
                    PdfAttachment {
                        mime: "image/png".into(),
                        bytes: encode_png_with_alpha(),
                    },
                ],
            },
            PdfItem {
                item: item(2, "DKK", "Rejse"),
                attachments: vec![
                    PdfAttachment {
                        mime: "application/pdf".into(),
                        bytes: attached_pdf,
                    },
                    PdfAttachment {
                        mime: "image/heic".into(),
                        bytes: vec![0; 10],
                    },
                    PdfAttachment {
                        mime: "application/pdf".into(),
                        bytes: b"%PDF-broken".to_vec(),
                    },
                ],
            },
        ];
        let out = render(&sheet(items)).unwrap();
        let doc = Document::load_mem(&out).unwrap();
        // 1 summary + 2 images + attached pages + HEIC placeholder + broken-PDF placeholder
        assert_eq!(doc.get_pages().len(), 1 + 2 + attached_pages + 1 + 1);
    }
}
