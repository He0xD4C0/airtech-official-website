use std::path::Path;

use crate::{error::ApiError, services::media};

use super::AssetProbe;

#[derive(Clone, Copy)]
pub(super) struct AssetType {
    pub(super) mime: &'static str,
    pub(super) extension: &'static str,
    pub(super) image: bool,
}

pub(super) fn classify_asset(
    name: &str,
    probe: &AssetProbe,
    byte_size: u64,
    response_type: Option<&str>,
    declared_type: Option<&str>,
) -> Result<AssetType, ApiError> {
    if let Some(image) = media::sniff_media_type(probe.prefix()) {
        return Ok(AssetType {
            mime: image.mime,
            extension: image.extension,
            image: true,
        });
    }
    let extension = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(unsupported_asset)?;
    let asset = document_type(&extension).ok_or_else(unsupported_asset)?;
    if !valid_signature(asset.extension, probe, byte_size) {
        return Err(ApiError::new(
            axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Unsupported media type",
            "The Feishu attachment content does not match its supported file type.",
        ));
    }
    for supplied in [response_type, declared_type].into_iter().flatten() {
        let supplied = supplied.split(';').next().unwrap_or(supplied).trim();
        if supplied != "application/octet-stream"
            && !supplied.eq_ignore_ascii_case(asset.mime)
            && !compatible_office_type(asset.extension, supplied)
        {
            return Err(ApiError::conflict(
                "Feishu attachment MIME metadata conflicts with its validated content.",
            ));
        }
    }
    Ok(asset)
}

fn document_type(extension: &str) -> Option<AssetType> {
    let (mime, extension) = match extension {
        "pdf" => ("application/pdf", "pdf"),
        "doc" => ("application/msword", "doc"),
        "docx" => (
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "docx",
        ),
        "xls" => ("application/vnd.ms-excel", "xls"),
        "xlsx" => (
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "xlsx",
        ),
        "dxf" => ("image/vnd.dxf", "dxf"),
        "dwg" => ("image/vnd.dwg", "dwg"),
        "step" => ("model/step", "step"),
        "stp" => ("model/step", "stp"),
        "iges" => ("model/iges", "iges"),
        "igs" => ("model/iges", "igs"),
        "stl" => ("model/stl", "stl"),
        "obj" => ("model/obj", "obj"),
        "3ds" => ("application/x-3ds", "3ds"),
        "sat" => ("application/octet-stream", "sat"),
        "prt" => ("application/octet-stream", "prt"),
        "sldprt" => ("application/octet-stream", "sldprt"),
        "asm" => ("application/octet-stream", "asm"),
        "sldasm" => ("application/octet-stream", "sldasm"),
        _ => return None,
    };
    Some(AssetType {
        mime,
        extension,
        image: false,
    })
}

fn valid_signature(extension: &str, probe: &AssetProbe, byte_size: u64) -> bool {
    let bytes = probe.prefix();
    match extension {
        "pdf" => bytes.starts_with(b"%PDF-"),
        "doc" | "xls" => bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0]),
        "docx" => bytes.starts_with(b"PK\x03\x04") && probe.has_office_path(b"word/"),
        "xlsx" => bytes.starts_with(b"PK\x03\x04") && probe.has_office_path(b"xl/"),
        "dwg" => bytes.starts_with(b"AC10"),
        "dxf" => bytes.starts_with(b"0\nSECTION") || bytes.starts_with(b"0\r\nSECTION"),
        "step" | "stp" => bytes.starts_with(b"ISO-10303-21"),
        "iges" | "igs" => byte_size >= 80,
        "stl" => bytes.starts_with(b"solid") || byte_size >= 84,
        "obj" => bytes.starts_with(b"#") || bytes.starts_with(b"v "),
        _ => byte_size > 0,
    }
}

pub(super) fn staged_attachment_error(error: std::io::Error) -> ApiError {
    tracing::error!(%error, "staged Feishu attachment could not be read");
    ApiError::service_unavailable("Feishu attachment staging is unavailable.")
}

fn compatible_office_type(extension: &str, supplied: &str) -> bool {
    matches!(extension, "docx" | "xlsx") && supplied == "application/zip"
}

fn unsupported_asset() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "Unsupported media type",
        "Only PNG, JPEG, WebP, PDF, Word, Excel, and supported CAD originals are accepted.",
    )
}

pub(super) fn sanitize_name(value: &str) -> String {
    let value = value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .chars()
        .filter(|character| !character.is_control())
        .take(180)
        .collect::<String>();
    if value.trim().is_empty() {
        "feishu-attachment".into()
    } else {
        value
    }
}
