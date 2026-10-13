#[test]
fn a_retained_line_opens_the_deepest_paragraph() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/retained_paragraphs.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()).trim(),
            row["html"].as_str().unwrap(),
            "{}",
            row["id"]
        );
    }
}
