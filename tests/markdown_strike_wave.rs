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

#[test]
fn underscores_beside_strike_delimiters_follow_native_gfm() {
    // Native cmark-gfm 0.29.0.gfm.13, revision 27d942c8, with -e strikethrough.
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/markdown_underscore_strike.json")).unwrap();
    let block_spacing =
        regex::Regex::new(r"\s*(</?(?:blockquote|p|ul|li|h[1-6])(?: [^>]*)?>)\s*").unwrap();
    let heading_id = regex::Regex::new(r#"(<h[1-6]) id="[^"]*""#).unwrap();
    let options = carve::Options::default().with_sections(false);
    for row in rows.as_array().unwrap() {
        let imported = carve::markdown_to_carve(row["source"].as_str().unwrap());
        let html = carve::to_html_with_options(&imported, &options);
        let html = heading_id.replace_all(&html, "$1");
        assert_eq!(
            block_spacing.replace_all(&html, "$1").trim(),
            row["html"].as_str().unwrap(),
            "{}: {imported}",
            row["id"]
        );
    }
}
