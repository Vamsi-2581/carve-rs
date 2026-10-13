#[test]
fn corpus_inline_slots_keep_tabs_as_content() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/space_only_inline_slots.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()).trim(),
            row["html"].as_str().unwrap(),
            "{}",
            row["id"]
        );
    }
}
