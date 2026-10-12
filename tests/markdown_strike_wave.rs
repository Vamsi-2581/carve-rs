use serde_json::Value;

#[test]
fn a_gfm_single_tilde_closes_before_a_letter() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/markdown_strike_wave.json")).unwrap();
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for row in rows.as_array().unwrap() {
        let imported = carve::markdown_to_carve(row["source"].as_str().unwrap());
        for html in [
            carve::to_html(&imported),
            carve::render_html(&carve::parse(&imported)).unwrap(),
        ] {
            assert_eq!(
                whitespace.replace_all(&html, "><").trim(),
                row["html"].as_str().unwrap(),
                "{}: {imported}",
                row["id"]
            );
        }
    }
}
