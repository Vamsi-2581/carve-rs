#[test]
fn a_descendants_rejected_fence_closer_is_not_reopened() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/descendant_fence_scope.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()).trim(),
            row["html"].as_str().unwrap(),
            "{}",
            row["id"]
        );
    }
}
