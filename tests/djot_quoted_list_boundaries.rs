#[test]
fn preserves_quote_and_list_ownership() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/djot-quoted-list-boundaries.json")).unwrap();
    let mut failures = Vec::new();
    for case in cases.as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let expected = case["carve"].as_str().unwrap();
        let imported = carve::djot_to_carve(source);
        let actual_html = carve::to_html(&imported);
        let expected_html = carve::to_html(expected);
        if actual_html != expected_html {
            failures.push(format!("{source:?}\nImported: {imported:?}\nActual: {actual_html}\nExpected: {expected_html}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn multiline_code_fallback_reports_plain_export_loss() {
    let result = carve::migrate_djot("a `b\n> c` d\n");
    let losses: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|row| row.code == "structure-unspellable")
        .collect();
    assert_eq!(losses.len(), 1);
    assert_eq!(losses[0].path.as_deref(), Some("line:1"));
    assert_eq!(
        losses[0].message,
        "Multiline inline code uses raw HTML; plain-text export omits it."
    );
}

#[test]
fn container_prefixes_do_not_report_code_loss() {
    for source in ["> a `b\n> c` d\n", "- > a `b\n  > c` d\n"] {
        let result = carve::migrate_djot(source);
        assert!(
            !result
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "structure-unspellable"),
            "{source:?}"
        );
    }
}

#[test]
fn output_growth_is_linear_for_wide_numeric_list_markers() {
    for size in [64, 128, 256] {
        let source = format!("{}1. > a\n{}", "0".repeat(size), " > b\n".repeat(size));
        let result = carve::djot_to_carve(&source);
        assert!(result.len() <= source.len() * 2);
        let expected = format!("1. > a\n{}", "   > b\n".repeat(size));
        assert_eq!(carve::to_html(&result), carve::to_html(&expected));
    }
}
