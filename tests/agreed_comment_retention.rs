#[test]
fn agreed_at_and_above_column_comments_retain_nothing() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/agreed_comment_retention.json")).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 36);
    let compact = regex::Regex::new(r">\s+<").unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            compact
                .replace_all(&carve::to_html(row["source"].as_str().unwrap()), "><")
                .trim(),
            row["html"].as_str().unwrap(),
            "{}",
            row["id"]
        );
    }
}
