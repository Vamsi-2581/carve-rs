//! PART 9 §16: an authored `{#id}` on a footnote reference replaces `fnref{n}` /
//! `fnref{n}-{k}`, and the note's backlink targets it.

use carve::Options;

const SOURCE: &str = "a [t[^1]{#z}][r] [^1]{#y} [^1]\n\n[r]: /u\n\n[^1]: n\n";

fn renders() -> [String; 2] {
    [
        carve::to_html(SOURCE),
        carve::to_html_with_options(SOURCE, &Options::new()),
    ]
}

#[test]
fn the_authored_id_replaces_the_generated_one() {
    for html in renders() {
        assert_eq!(
            html.lines().next().unwrap(),
            "<p>a <a href=\"/u\">t<sup id=\"z\">1</sup></a> <a id=\"y\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a> <a id=\"fnref1-3\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>",
        );
    }
}

#[test]
fn the_backlinks_follow_the_authored_ids() {
    for html in renders() {
        let hrefs: Vec<&str> = html
            .split("<a href=\"")
            .skip(1)
            .filter(|rest| rest.contains("role=\"doc-backlink\""))
            .filter_map(|rest| rest.split('"').next())
            .filter(|href| href.starts_with('#'))
            .collect();
        assert_eq!(hrefs, ["#z", "#y", "#fnref1-3"]);
    }
}

#[test]
fn an_authored_id_is_escaped_in_the_backlink() {
    let html = carve::to_html("a [^1]{id=\"q&r\"}\n\n[^1]: n\n");
    assert!(
        html.contains("<a id=\"q&amp;r\" href=\"#fn1\" role=\"doc-noteref\">"),
        "{html}"
    );
    assert!(
        html.contains("<a href=\"#q&amp;r\" role=\"doc-backlink\""),
        "{html}"
    );
}

#[test]
fn the_reference_inside_link_text_imports_as_a_footnote_again() {
    let html = carve::to_html(SOURCE);
    let back = carve::html_to_carve(&html, &carve::HtmlImportOptions::default())
        .unwrap()
        .value;
    assert_eq!(back, "a [t[^1]](/u) [^1] [^1]\n\n[^1]: n\n");
}
