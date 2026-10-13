#[test]
fn quoted_item_block_end_releases_the_tail() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/quoted_item_block_end.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()).trim(),
            row["html"].as_str().unwrap(),
            "{}",
            row["id"]
        );
    }
}
