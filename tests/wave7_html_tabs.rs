use carve::{html_to_carve, HtmlImportOptions};

#[test]
fn literal_paragraph_tabs_survive_html_import() {
    for text in ["[a]: \t/u", "a\tb", "a \t b", "a\t\tb"] {
        let source = format!("see [t][a]\n\n{text}\n");
        let html = carve::to_html(&source);
        let back = html_to_carve(&html, &HtmlImportOptions::default())
            .unwrap()
            .value;
        assert!(back.contains(text), "{text:?}: {back:?}");
        assert_eq!(carve::to_html(&back), html);
    }
    let back = html_to_carve("<p>a  b\n c</p>", &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert_eq!(back, "a b c\n");
}
