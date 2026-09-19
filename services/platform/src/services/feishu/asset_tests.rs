use super::{classify_asset, sanitize_name};
use crate::services::feishu::client::AssetProbe;

fn classify(
    name: &str,
    bytes: &[u8],
    response_type: Option<&str>,
) -> Result<super::AssetType, crate::error::ApiError> {
    classify_asset(
        name,
        &AssetProbe::from_bytes(bytes),
        bytes.len() as u64,
        response_type,
        None,
    )
}

#[test]
fn validates_supported_documents_and_cad_by_content() {
    let pdf = classify("catalog.pdf", b"%PDF-1.7\n", Some("application/pdf")).expect("valid PDF");
    assert_eq!(pdf.mime, "application/pdf");
    assert!(!pdf.image);

    let step =
        classify("fan.step", b"ISO-10303-21;\nHEADER;", Some("model/step")).expect("valid STEP");
    assert_eq!(step.extension, "step");
}

#[test]
fn distinguishes_word_and_excel_zip_packages() {
    let word = b"PK\x03\x04mock-word/document.xml";
    assert!(classify("drawing.docx", word, Some("application/zip")).is_ok());
    assert!(classify("drawing.xlsx", word, Some("application/zip")).is_err());

    let excel = b"PK\x03\x04mock-xl/workbook.xml";
    assert!(classify("curve.xlsx", excel, Some("application/zip")).is_ok());
    assert!(classify("curve.docx", excel, Some("application/zip")).is_err());
}

#[test]
fn rejects_conflicting_mime_and_sanitizes_source_names() {
    assert!(classify("catalog.pdf", b"%PDF-1.7\n", Some("text/html")).is_err());
    assert_eq!(sanitize_name("../source\r\ncurve.xlsx"), "sourcecurve.xlsx");
}
