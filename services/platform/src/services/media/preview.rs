use std::{
    io::Cursor,
    sync::{Arc, OnceLock},
    time::Duration,
};

use image::{imageops::FilterType, DynamicImage, GenericImageView};
use tokio::sync::Semaphore;

use crate::error::{ApiError, MEDIA_DECODE_FAILED};

const MAX_IMAGE_EDGE: u32 = 16_384;
const MAX_IMAGE_PIXELS: u64 = 40_000_000;
const PREVIEW_EDGE: u32 = 1_600;
const PREVIEW_QUALITY: f32 = 82.0;
const PROCESSING_TIMEOUT: Duration = Duration::from_secs(30);
const PROCESSING_CONCURRENCY: usize = 2;

static IMAGE_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();

#[derive(Debug)]
pub struct PreviewDerivative {
    pub bytes: Vec<u8>,
    pub original_width: u32,
    pub original_height: u32,
    pub width: u32,
    pub height: u32,
}

pub async fn generate_preview(bytes: Vec<u8>) -> Result<PreviewDerivative, ApiError> {
    let slots = IMAGE_SLOTS
        .get_or_init(|| Arc::new(Semaphore::new(PROCESSING_CONCURRENCY)))
        .clone();
    tokio::time::timeout(PROCESSING_TIMEOUT, async move {
        let _permit = slots
            .acquire_owned()
            .await
            .map_err(|_| processing_unavailable())?;
        tokio::task::spawn_blocking(move || process_image(&bytes))
            .await
            .map_err(|error| {
                tracing::error!(%error, "media preview worker failed");
                processing_unavailable()
            })?
    })
    .await
    .map_err(|_| decode_error("Image processing exceeded the 30 second deadline."))?
}

fn process_image(bytes: &[u8]) -> Result<PreviewDerivative, ApiError> {
    let decoded = image::load_from_memory(bytes).map_err(|_| {
        decode_error("The uploaded file is not a complete PNG, JPEG, or WebP image.")
    })?;
    let (raw_width, raw_height) = decoded.dimensions();
    validate_dimensions(raw_width, raw_height)?;
    let oriented = apply_orientation(decoded, exif_orientation(bytes));
    let (original_width, original_height) = oriented.dimensions();
    validate_dimensions(original_width, original_height)?;
    let preview = if original_width.max(original_height) > PREVIEW_EDGE {
        oriented.resize(PREVIEW_EDGE, PREVIEW_EDGE, FilterType::Lanczos3)
    } else {
        oriented
    };
    let (width, height) = preview.dimensions();
    // The WebP crate only accepts RGB8/RGBA8 DynamicImage variants. Feishu
    // drawings also contain valid grayscale PNGs, so normalize every decoded
    // image to RGBA before encoding while preserving any alpha channel.
    let rgba = preview.to_rgba8();
    let encoded = webp::Encoder::from_rgba(rgba.as_raw(), width, height)
        .encode(PREVIEW_QUALITY)
        .to_vec();
    if encoded.is_empty() {
        return Err(processing_unavailable());
    }
    Ok(PreviewDerivative {
        bytes: encoded,
        original_width,
        original_height,
        width,
        height,
    })
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), ApiError> {
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0
        || height == 0
        || width > MAX_IMAGE_EDGE
        || height > MAX_IMAGE_EDGE
        || pixels > MAX_IMAGE_PIXELS
    {
        return Err(decode_error(
            "Images must be at most 16,384 px per side and 40 megapixels.",
        ));
    }
    Ok(())
}

fn exif_orientation(bytes: &[u8]) -> u32 {
    exif::Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()
        .and_then(|exif| {
            exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                .cloned()
        })
        .and_then(|field| field.value.get_uint(0))
        .unwrap_or(1)
}

fn apply_orientation(image: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => image.fliph(),
        3 => image.rotate180(),
        4 => image.flipv(),
        5 => image.rotate90().fliph(),
        6 => image.rotate90(),
        7 => image.rotate270().fliph(),
        8 => image.rotate270(),
        _ => image,
    }
}

fn decode_error(detail: &str) -> ApiError {
    ApiError::new(
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "Image decode failed",
        detail,
    )
    .with_code(MEDIA_DECODE_FAILED)
}

fn processing_unavailable() -> ApiError {
    ApiError::service_unavailable("Media preview generation is temporarily unavailable.")
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, ImageBuffer, Luma, Rgba};

    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            width,
            height,
            Rgba([20, 80, 120, 96]),
        ));
        let mut bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn creates_bounded_webp_without_upscaling_and_preserves_alpha() {
        let small = process_image(&png(80, 40)).unwrap();
        assert_eq!((small.original_width, small.original_height), (80, 40));
        assert_eq!((small.width, small.height), (80, 40));
        let decoded =
            image::load_from_memory_with_format(&small.bytes, image::ImageFormat::WebP).unwrap();
        assert!(decoded.color().has_alpha());

        let large = process_image(&png(2_000, 1_000)).unwrap();
        assert_eq!((large.width, large.height), (1_600, 800));
    }

    #[test]
    fn rejects_corrupt_and_over_limit_images() {
        assert_eq!(process_image(b"not an image").unwrap_err().status(), 422);
        assert!(validate_dimensions(16_385, 1).is_err());
        assert!(validate_dimensions(10_000, 5_000).is_err());
    }

    #[test]
    fn creates_preview_for_grayscale_pngs() {
        let image = DynamicImage::ImageLuma8(ImageBuffer::from_pixel(40, 20, Luma([80])));
        let mut bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        let preview = process_image(&bytes).unwrap();
        assert_eq!((preview.width, preview.height), (40, 20));
        assert!(!preview.bytes.is_empty());
    }
}
