use serde_json::Value;
#[test]
fn matches_checker() {
    let mut rows: Value =
        serde_json::from_str(include_str!("fixtures/term_led_description_code.json")).unwrap();
    let controls: Value =
        serde_json::from_str(include_str!("fixtures/term_lazy_description_review.json")).unwrap();
    rows.as_array_mut()
        .unwrap()
        .extend(controls.as_array().unwrap().iter().cloned());
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        for html in [
            carve::to_html(source),
            carve::render_html(&carve::parse(source)).unwrap(),
            carve::to_html_with_options(source, &carve::Options::default().with_positions(true)),
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
