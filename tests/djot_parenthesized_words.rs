use serde_json::Value;
#[test]
fn matches_checker() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/djot_parenthesized_words.json")).unwrap();
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let imported = carve::djot_to_carve(source);
        for html in [
            carve::to_html(&imported),
            carve::render_html(&carve::parse(&imported)).unwrap(),
        ] {
            assert_eq!(
                whitespace.replace_all(&html, "><").trim(),
                row["html"].as_str().unwrap(),
                "{}: {source}",
                row["id"]
            );
        }
    }
}
