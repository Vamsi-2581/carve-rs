//! Under roundtrip, an HTML import consumes an invalid generic-span carrier the
//! way safe and semantic do (CARVE-P10-021), instead of keeping the span as raw
//! HTML with the reserved name inside.

use carve::{
    html_to_ast, html_to_carve, to_html, HtmlImportDiagnosticCode, HtmlImportMode,
    HtmlImportOptions,
};

fn roundtrip() -> HtmlImportOptions {
    options(HtmlImportMode::Roundtrip)
}

fn options(mode: HtmlImportMode) -> HtmlImportOptions {
    HtmlImportOptions {
        mode,
        ..Default::default()
    }
}

fn dropped(html: &str, options: &HtmlImportOptions) -> usize {
    html_to_ast(html, options)
        .unwrap()
        .report
        .diagnostics
        .iter()
        .filter(|d| d.code == HtmlImportDiagnosticCode::AttributeDropped)
        .count()
}

#[test]
fn an_invalid_span_carrier_is_consumed_under_roundtrip() {
    for (html, text) in [
        ("<p><span data-carve-raw=\"x y\">z</span></p>", "z\n"),
        ("<p><span data-carve-escaped>ab</span></p>", "ab\n"),
        (
            "<p><span data-carve-escaped data-carve-raw=\"latex\">*</span></p>",
            "*\n",
        ),
    ] {
        let source = html_to_carve(html, &roundtrip()).unwrap().value;
        assert_eq!(source, text, "{html}");
        assert_eq!(
            to_html(&source),
            to_html(
                &html_to_carve(html, &options(HtmlImportMode::Safe))
                    .unwrap()
                    .value
            )
        );
        assert_eq!(
            dropped(html, &roundtrip()),
            dropped(html, &options(HtmlImportMode::Safe)),
            "{html}"
        );
    }
    assert_eq!(
        dropped("<p><span data-carve-raw=\"x y\">z</span></p>", &roundtrip()),
        1
    );
}

#[test]
fn a_span_without_a_reserved_name_keeps_its_roundtrip_reading() {
    let source = html_to_carve("<p><span>z</span></p>", &roundtrip())
        .unwrap()
        .value;
    assert_eq!(source, "`<span>z</span>`{=html}\n");
}
