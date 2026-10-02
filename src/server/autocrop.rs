//! Finds a document (receipt, invoice) in a photo, straightens its perspective and crops to it.

use std::io::Cursor;

use image::{DynamicImage, GrayImage, Rgb, RgbImage, codecs::jpeg::JpegEncoder, imageops::FilterType};
use imageproc::{
    contours::{BorderType, find_contours},
    contrast::{ThresholdType, otsu_level, threshold},
    distance_transform::Norm,
    edges::canny,
    filter::gaussian_blur_f32,
    geometric_transformations::{Border, Interpolation, Projection, warp_into},
    geometry::{approximate_polygon_dp, arc_length, contour_area, convex_hull},
    morphology::{close, dilate},
    point::Point,
};

use super::pdf::jpeg_orientation;

/// Detection runs on a downscaled copy; the warp uses the full-resolution image.
const DETECT_MAX_SIDE: u32 = 640;
/// The document must cover a reasonable part of the photo to count as found.
const MIN_AREA_RATIO: f64 = 0.12;
/// Above this the photo is effectively already cropped.
const MAX_AREA_RATIO: f64 = 0.92;
const MIN_OUTPUT_SIDE: f32 = 120.0;
const JPEG_QUALITY: u8 = 88;

/// Corners ordered top-left, top-right, bottom-right, bottom-left.
type Quad = [(f32, f32); 4];

fn order_corners(points: &[Point<i32>]) -> Option<Quad> {
    let key = |p: &&Point<i32>, f: fn(i32, i32) -> i32| f(p.x, p.y);
    let tl = points.iter().min_by_key(|p| key(p, |x, y| x + y))?;
    let br = points.iter().max_by_key(|p| key(p, |x, y| x + y))?;
    let tr = points.iter().min_by_key(|p| key(p, |x, y| y - x))?;
    let bl = points.iter().max_by_key(|p| key(p, |x, y| y - x))?;
    let quad = [tl, tr, br, bl].map(|p| (p.x as f32, p.y as f32));
    let distinct = (0..4).all(|i| (i + 1..4).all(|j| quad[i] != quad[j]));
    distinct.then_some(quad)
}

/// Rejects slivers and strongly distorted shapes: every corner must be roughly 45–135 degrees.
fn plausible(q: &Quad) -> bool {
    (0..4).all(|i| {
        let (p, a, b) = (q[i], q[(i + 3) % 4], q[(i + 1) % 4]);
        let (ux, uy, vx, vy) = (a.0 - p.0, a.1 - p.1, b.0 - p.0, b.1 - p.1);
        let len = (ux.hypot(uy) * vx.hypot(vy)).max(f32::EPSILON);
        ((ux * vx + uy * vy) / len).abs() < 0.71
    })
}

/// Largest plausible four-cornered outline in a binary image.
fn best_quad(binary: &GrayImage) -> Option<Quad> {
    let total = (binary.width() * binary.height()) as f64;
    let mut best: Option<(f64, Quad)> = None;
    for contour in find_contours::<i32>(binary) {
        if contour.border_type != BorderType::Outer || contour.points.len() < 20 {
            continue;
        }
        let hull = convex_hull(contour.points);
        let area = contour_area(&hull);
        if area < total * MIN_AREA_RATIO || area > total * MAX_AREA_RATIO {
            continue;
        }
        if best.as_ref().is_some_and(|(a, _)| *a >= area) {
            continue;
        }
        let perimeter = arc_length(&hull, true);
        // Loosen the tolerance until the outline collapses to four corners.
        let corners = [0.02, 0.03, 0.045, 0.06, 0.08].into_iter().find_map(|f| {
            let mut poly = approximate_polygon_dp(&hull, perimeter * f, true);
            if poly.len() > 1 && poly.first() == poly.last() {
                poly.pop();
            }
            (poly.len() == 4).then_some(poly)
        });
        let Some(corners) = corners else { continue };
        // The four corners must account for most of the outline, or it isn't a sheet.
        if contour_area(&corners) < area * 0.85 {
            continue;
        }
        if let Some(quad) = order_corners(&corners).filter(plausible) {
            best = Some((area, quad));
        }
    }
    best.map(|(_, q)| q)
}

/// Looks for the document outline in a small grayscale image.
fn find_document(gray: &GrayImage) -> Option<Quad> {
    let blurred = gaussian_blur_f32(gray, 2.0);

    // Paper is usually lighter than the table it lies on.
    let level = otsu_level(&blurred);
    let light = close(&threshold(&blurred, level, ThresholdType::Binary), Norm::LInf, 3);
    if let Some(q) = best_quad(&light) {
        return Some(q);
    }
    let dark = close(&threshold(&blurred, level, ThresholdType::BinaryInverted), Norm::LInf, 3);
    if let Some(q) = best_quad(&dark) {
        return Some(q);
    }
    // Low contrast: fall back to edges, thickened so the outline is closed.
    best_quad(&dilate(&canny(&blurred, 20.0, 60.0), Norm::LInf, 2))
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// Returns the straightened, cropped document as JPEG, or `None` when no document was found.
pub fn auto_crop(bytes: &[u8], mime: &str) -> Option<Vec<u8>> {
    let mut img = image::load_from_memory(bytes).ok()?;
    if mime == "image/jpeg" {
        img.apply_orientation(jpeg_orientation(bytes));
    }
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let scale = (DETECT_MAX_SIDE as f32 / w.max(h) as f32).min(1.0);
    let small = image::imageops::resize(
        &DynamicImage::ImageRgb8(rgb.clone()).to_luma8(),
        ((w as f32 * scale).round() as u32).max(1),
        ((h as f32 * scale).round() as u32).max(1),
        FilterType::Triangle,
    );

    let quad = find_document(&small)?.map(|(x, y)| (x / scale, y / scale));
    let [tl, tr, br, bl] = quad;
    let out_w = distance(tl, tr).max(distance(bl, br)).round();
    let out_h = distance(tl, bl).max(distance(tr, br)).round();
    if out_w < MIN_OUTPUT_SIDE || out_h < MIN_OUTPUT_SIDE {
        return None;
    }

    let projection = Projection::from_control_points(quad, [(0.0, 0.0), (out_w, 0.0), (out_w, out_h), (0.0, out_h)])?;
    let mut out = RgbImage::new(out_w as u32, out_h as u32);
    warp_into(&rgb, projection, Interpolation::Bilinear, Border::Constant(Rgb([255, 255, 255])), &mut out);

    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(Cursor::new(&mut jpeg), JPEG_QUALITY).encode_image(&out).ok()?;
    Some(jpeg)
}

#[cfg(test)]
mod tests {
    use image::ImageFormat;
    use imageproc::drawing::{draw_filled_rect_mut, draw_polygon_mut};
    use imageproc::rect::Rect;

    use super::*;

    fn png(img: &RgbImage) -> Vec<u8> {
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(img.clone()).write_to(&mut Cursor::new(&mut out), ImageFormat::Png).unwrap();
        out
    }

    /// A tilted, perspective-distorted white "receipt" with dark text lines on a dark table.
    fn photo() -> RgbImage {
        let mut img = RgbImage::from_pixel(1000, 750, Rgb([60, 55, 50]));
        let corners = [Point::new(260, 110), Point::new(700, 160), Point::new(650, 680), Point::new(210, 630)];
        draw_polygon_mut(&mut img, &corners, Rgb([235, 235, 230]));
        for i in 0..8 {
            draw_filled_rect_mut(&mut img, Rect::at(320, 230 + i * 45).of_size(250, 8), Rgb([30, 30, 30]));
        }
        img
    }

    #[test]
    fn crops_and_straightens_a_receipt() {
        let out = auto_crop(&png(&photo()), "image/png").expect("document found");
        let out = image::load_from_memory(&out).unwrap().to_rgb8();
        let (w, h) = out.dimensions();
        // Expected: width ≈ 443 px (top edge), height ≈ 522 px (right edge).
        assert!((420..=470).contains(&w), "width {w}");
        assert!((495..=550).contains(&h), "height {h}");
        // No table left in the corners or along the edges' midpoints.
        for (x, y) in [(6, 6), (w - 7, 6), (w - 7, h - 7), (6, h - 7), (w / 2, 4), (4, h / 2)] {
            let p = out.get_pixel(x, y);
            assert!(p[0] > 150, "pixel at ({x},{y}) is {p:?}");
        }
    }

    #[test]
    fn leaves_photos_without_a_document_alone() {
        let plain = RgbImage::from_pixel(800, 600, Rgb([120, 120, 120]));
        assert!(auto_crop(&png(&plain), "image/png").is_none());

        // A sheet that already fills the frame needs no cropping.
        let mut full = RgbImage::from_pixel(800, 600, Rgb([240, 240, 240]));
        draw_filled_rect_mut(&mut full, Rect::at(0, 0).of_size(800, 6), Rgb([40, 40, 40]));
        assert!(auto_crop(&png(&full), "image/png").is_none());
    }

    #[test]
    fn orders_corners() {
        let pts = [Point::new(90, 10), Point::new(5, 8), Point::new(10, 95), Point::new(100, 90)];
        assert_eq!(order_corners(&pts), Some([(5.0, 8.0), (90.0, 10.0), (100.0, 90.0), (10.0, 95.0)]));
    }
}
