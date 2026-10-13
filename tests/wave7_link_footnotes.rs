use carve::{html_to_carve, HtmlImportOptions};

#[test]
fn imported_link_footnotes_keep_their_nesting_and_attributes() {
    for source in [
        "a [t[^1]](/u) b\n\n[^1]: n\n",
        "a [t[^note]](/u) b\n\n[^note]: n\n",
        "a [t[^1]{.x}](/u) b\n\n[^1]: n\n",
        "a [t](/u)[^1] b\n\n[^1]: n\n",
    ] {
        let doc = carve::parse(source);
        let html = carve::render_html(&doc).unwrap();
        let back = html_to_carve(&html, &HtmlImportOptions::default())
            .unwrap()
            .value;
        assert_eq!(carve::to_html(&back), html, "{source:?}: {back:?}");
        let roundtrip =
            carve::render_html_with_options(&doc, &carve::Options::default().with_roundtrip(true))
                .unwrap();
        let back = html_to_carve(&roundtrip, &HtmlImportOptions::default())
            .unwrap()
            .value;
        assert_eq!(carve::to_html(&back), html, "{source:?}: {back:?}");
        if source.contains("note") {
            assert!(back.contains("[^note]"), "{back}");
        }
    }
}

#[test]
fn a_sup_without_an_endnotes_backlink_remains_a_sup() {
    let html = "<p><a href=\"/u\">t<sup id=\"fnref1\">1</sup></a></p>";
    let back = html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert!(back.contains("{^1^}"));
    assert!(!back.contains("[^"));
}

#[test]
fn imported_corpus_link_footnotes_keep_the_tree() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/wave7_link_footnotes.json")).unwrap();
    let carriers = regex::Regex::new(r#" data-carve-footnote-label="[^"]*""#).unwrap();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let html = carve::to_html(source);
        assert_eq!(html.trim(), row["html"].as_str().unwrap());
        let back = html_to_carve(&html, &HtmlImportOptions::default())
            .unwrap()
            .value;
        assert_eq!(
            carriers.replace_all(&carve::to_html(&back), ""),
            carriers.replace_all(&html, ""),
            "{}: {back:?}",
            row["id"]
        );
    }
}

#[test]
fn bare_footnote_sites_keep_shared_wrapper_notes_separate() {
    let html = r##"<p><a href="/u">t<sup id="fnref1">1</sup><sup id="fnref2">2</sup></a></p>
<section role="doc-endnotes"><div id="notes">
<p id="fn1">first <a href="#fnref1" role="doc-backlink">back</a></p>
<p id="fn2">second <a href="#fnref2" role="doc-backlink">back</a></p>
</div></section>"##;
    let back = html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    let doc = carve::parse(&back);
    assert_eq!(doc.footnote_defs.len(), 2, "{back}");
    let rendered = carve::render_html(&doc).unwrap();
    assert_eq!(rendered.matches("first").count(), 1, "{back}");
    assert_eq!(rendered.matches("second").count(), 1, "{back}");
    assert!(back.contains("[^1]: first"), "{back}");
    assert!(back.contains("[^2]: second"), "{back}");
}

#[test]
fn an_endnotes_section_id_does_not_define_a_bare_site_note() {
    let html = r##"<p><a href="/u">t<sup id="fnref1">1</sup></a></p>
<section id="notes" role="doc-endnotes"><p>first <a href="#fnref1" role="doc-backlink">back</a></p></section>"##;
    let back = html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert!(!back.contains("[^"), "{back}");
}

#[test]
fn a_collection_wrapper_is_not_a_note_body() {
    let html = r##"<p><a href="/u">t<sup id="fnref1">1</sup></a></p>
<div id="notes" role="doc-endnotes"><p>Notes heading</p>
<p id="fn1">first <a href="#fnref1" role="doc-backlink">back</a></p></div>"##;
    let back = html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert!(back.contains("[^1]: first"), "{back}");
    assert!(!back.contains("[^1]: Notes"), "{back}");
}

#[test]
fn a_shared_target_cannot_bind_two_distinct_bare_note_numbers() {
    let html = r##"<p><a href="/u">t<sup id="fnref1">1</sup><sup id="fnref2">2</sup></a></p>
<section role="doc-endnotes"><div id="notes">
<p>first <a href="#fnref1" role="doc-backlink">back</a></p>
<p>second <a href="#fnref2" role="doc-backlink">back</a></p>
</div></section>"##;
    let back = html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert!(!back.contains("[^"), "{back}");
}

#[test]
fn repeated_bare_references_to_one_note_share_the_definition() {
    let source = "[t[^1][^1]](/u)\n\n[^1]: first\n";
    let html = carve::to_html(source);
    let back = html_to_carve(&html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert_eq!(carve::to_html(&back), html);
    assert_eq!(carve::parse(&back).footnote_defs.len(), 1);
}

#[test]
fn an_unidentified_note_cannot_absorb_its_identified_sibling() {
    let html = r##"<p><a href="/u">t<sup id="fnref1">1</sup><sup id="fnref2">2</sup></a></p>
<section role="doc-endnotes"><div id="notes">
<p id="fn1">first <a href="#fnref1" role="doc-backlink">back</a></p>
<p>second <a href="#fnref2" role="doc-backlink">back</a></p>
</div></section>"##;
    let back = html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert!(back.contains("[^1]: first"), "{back}");
    assert!(!back.contains("[^2]:"), "{back}");
    assert_eq!(carve::to_html(&back).matches("first").count(), 1);
}

#[test]
fn a_bare_site_wrapper_cannot_absorb_an_anchor_site_definition() {
    let html = r##"<p><a href="/u">t<sup id="fnref1">1</sup></a>
 x<a href="#fn3" id="fnref3" role="doc-noteref"><sup>3</sup></a></p>
<section role="doc-endnotes"><div id="notes">
<p>first <a href="#fnref1" role="doc-backlink">back</a></p>
<p id="fn3">third <a href="#fnref3" role="doc-backlink">back</a></p>
</div></section>"##;
    let back = html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert_eq!(carve::parse(&back).footnote_defs.len(), 1, "{back}");
    assert!(back.contains(": third"), "{back}");
    assert_eq!(carve::to_html(&back).matches("third").count(), 1, "{back}");
}

#[test]
fn a_reference_in_another_note_body_is_not_a_definition_target() {
    let source = "[t[^1]](/u)\n\n[^1]: [u[^2]](/v)\n[^2]: second\n";
    let html = carve::to_html(source);
    let back = html_to_carve(&html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert_eq!(carve::to_html(&back), html, "{back}");
}
