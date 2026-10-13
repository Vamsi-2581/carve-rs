use carve::CheckedRenderOptions;

#[test]
fn unused_definition_destination_follows_the_plain_text_oracle() {
    let source = include_str!("spec/tests/corpus/108-security-hardening-12.crv");
    let expected = include_str!("spec/tests/corpus/108-security-hardening-12.txt");
    let result = carve::to_plain_text_with_report(source, CheckedRenderOptions::default()).unwrap();

    assert_eq!(result.value, expected);
    assert_eq!(result.total_losses, 1);
    assert_eq!(result.losses.len(), 1);
    assert_eq!(result.losses[0].code, "destination-denied");
    assert_eq!(result.losses[0].pos.as_ref().unwrap().start_line, 5);
}
