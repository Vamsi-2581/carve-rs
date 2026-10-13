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

#[test]
fn typography_uses_one_flank_class() {
    for flank in ['\u{000b}', '\u{000c}', '\u{0085}', '\u{2003}'] {
        assert_eq!(carve::to_html(&format!("x{flank}\"q\"")), format!("<p>x{flank}”q”</p>"));
    }
    for flank in [' ', '\t', '\u{00a0}'] {
        let rendered = if flank == '\u{00a0}' { "&nbsp;".to_owned() } else { flank.to_string() };
        assert_eq!(carve::to_html(&format!("x{flank}\"q\"")), format!("<p>x{rendered}“q”</p>"));
    }
}

#[test]
fn a_tab_before_a_trailing_comment_stays_content() {
    corpus_category("518-");
    corpus_category("516-");
    assert_eq!(carve::to_html("a\t %% c"), "<p>a\t</p>");
}

#[test]
fn a_colon_opener_on_an_item_marker_line_opens() {
    corpus_category("161-");
}
