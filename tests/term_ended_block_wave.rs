use serde_json::Value;

#[test]
fn a_term_ending_block_leaves_the_next_opener_outside_the_term() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/term_ended_block_wave.json")).unwrap();
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        for html in [
            carve::to_html(source),
            carve::render_html(&carve::parse(source)).unwrap(),
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
