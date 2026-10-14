#[test]
fn unclosed_code_with_colon_line_formats_once() {
    let source = "`\n :::\nc\n";
    let first = carve::to_carve(source);
    assert_eq!(carve::to_carve(&first), first);
    assert_eq!(carve::to_html(&first), carve::to_html(source));

    assert_eq!(carve::to_carve("`x`\n"), "`x`\n");
}

#[test]
fn caption_code_with_an_indented_line_can_be_written() {
    let source = "||\n^ `\n ^\n";
    let formatted = carve::try_to_carve_with_options(source, &carve::Options::default())
        .expect("the parsed caption has a Carve spelling");
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
    assert_eq!(carve::to_carve(&formatted), formatted);

    let control = "||\n^ `\n^\n";
    assert!(carve::try_to_carve_with_options(control, &carve::Options::default()).is_ok());
}
