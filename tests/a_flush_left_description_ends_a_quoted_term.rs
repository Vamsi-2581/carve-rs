//! A flush-left `: ` line ends an open term (CARVE-P2-028). Below a quoted
//! item whose only content is that term, the line carries no `>`, so it opens
//! nothing inside the quote and is read where it stands.

use carve::to_html;

fn compact(html: &str) -> String {
    html.split_whitespace().collect()
}

#[test]
fn a_flush_left_description_after_a_quoted_term_only_item_leaves_the_quote() {
    for marker in ["-", "1.", "a.", "iv)", "- [ ]", "-{.c}"] {
        let html = compact(&to_html(&format!("> {marker} :: term\n: body\n")));
        assert!(
            html.ends_with("</blockquote><p>:body</p>"),
            "{marker}: {html}"
        );
        assert!(!html.contains("<dd>"), "{marker}: {html}");
    }
}

#[test]
fn one_level_deeper_the_line_is_text_of_the_outer_item() {
    let html = compact(&to_html("- > - :: t\n  : body\n"));
    assert_eq!(
        html,
        "<ul><li><blockquote><ul><li><dl><dt>t</dt></dl></li></ul></blockquote>:body</li></ul>"
    );
}

#[test]
fn a_description_at_the_item_content_column_still_attaches() {
    let html = compact(&to_html("- :: term\n  : body\n"));
    assert!(html.contains("<dd>body</dd>"), "{html}");
    let html = compact(&to_html("> - :: term\n  : body\n"));
    assert!(html.contains("<dd>body</dd>"), "{html}");
}
