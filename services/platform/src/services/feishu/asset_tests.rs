use super::{classify_asset, sanitize_name};

#[test]
fn validates_supported_documents_and_cad_by_content() {
    let pdf = classify_asset("catalog.pdf", b"%PDF-1.7\n", Some("application/pdf"), None)
        .expect("valid PDF");
    assert_eq!(pdf.mime, "application/pdf");
    assert!(!pdf.image);

    let step = classify_asset(
        "fan.step",
        b"ISO-10303-21;\nHEADER;",
        Some("model/step"),
        None,
    )
    .expect("valid STEP");
    assert_eq!(step.extension, "step");
}

#[test]
fn distinguishes_word_and_excel_zip_packages() {
    let word = b"PK\x03\x04mock-word/document.xml";
    assert!(classify_asset("drawing.docx", word, Some("application/zip"), None).is_ok());
    assert!(classify_asset("drawing.xlsx", word, Some("application/zip"), None).is_err());

    let excel = b"PK\x03\x04mock-xl/workbook.xml";
    assert!(classify_asset("curve.xlsx", excel, Some("application/zip"), None).is_ok());
    assert!(classify_asset("curve.docx", excel, Some("application/zip"), None).is_err());
}

#[test]
fn rejects_conflicting_mime_and_sanitizes_source_names() {
    assert!(classify_asset("catalog.pdf", b"%PDF-1.7\n", Some("text/html"), None).is_err());
    assert_eq!(sanitize_name("../source\r\ncurve.xlsx"), "sourcecurve.xlsx");
}
