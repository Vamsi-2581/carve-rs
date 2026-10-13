#[test]
fn unfinished_nested_fences_release_below_column_lines() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/unfinished_nested_fences.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()).trim(),
            row["html"].as_str().unwrap(),
            "{}",
            row["id"]
        );
    }
}

#[test]
fn a_marker_in_an_open_description_paragraph_is_inline_text() {
    let source = "> :: t\n> :  d\n>    - ```\n>      x\nflush\n";
    let expected = include_str!("spec/tests/corpus/514-a-fence-a-container-inside-a-quote-holds-open-stores-no-claim-12.html");
    assert_eq!(carve::to_html(source).trim(), expected.trim());
}

#[test]
fn a_released_fence_at_the_ancestor_column_opens_its_own_block() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/nested_review_controls.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()).trim(),
            row["html"].as_str().unwrap().trim(),
            "{}",
            row["id"]
        );
    }
}
