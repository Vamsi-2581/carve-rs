#[test]
fn term_led_description_keeps_its_code() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/term_description_code.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()).trim(),
            row["html"].as_str().unwrap(),
            "{}",
            row["id"]
        );
    }
}
