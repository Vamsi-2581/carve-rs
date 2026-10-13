fn corpus_category(prefix: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    let mut count = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "crv")
            && path.file_name().unwrap().to_string_lossy().starts_with(prefix)
        {
            let source = std::fs::read_to_string(&path).unwrap();
            let expected = std::fs::read_to_string(path.with_extension("html")).unwrap();
            assert_eq!(carve::to_html(&source).trim(), expected.trim(), "{}", path.display());
            count += 1;
        }
    }
    assert!(count > 0, "missing corpus category {prefix}");
}

#[test]
fn a_note_reference_inside_link_text_keeps_its_sup_id() {
    corpus_category("573-");
}
