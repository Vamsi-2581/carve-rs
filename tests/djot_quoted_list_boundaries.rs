#[test]
fn preserves_quote_and_list_ownership() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/djot-quoted-list-boundaries.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let expected = case["carve"].as_str().unwrap();
        assert_eq!(
            carve::to_html(&carve::djot_to_carve(source)),
            carve::to_html(expected),
            "{source:?}"
        );
    }
}
