#[test]
fn djot_attribute_ownership_survives_import() {
    for (source, expected) in [
        ("{#id} at beginning\n", "<p> at beginning</p>"),
        ("After {#id} space\n{.class}\n", "<p>After  space\n</p>"),
        ("not a [span] {#id}.\n", "<p>not a [span] .</p>"),
        (
            "{#id .class}\n\nA paragraph\n",
            "<p id=\"id\" class=\"class\">A paragraph</p>",
        ),
        ("[span]{#id}", "<p><span id=\"id\">span</span></p>"),
        ("`x`{.c}", "<p><code class=\"c\">x</code></p>"),
        (
            "<http://x.y>{.c}",
            "<p><a href=\"http://x.y\" class=\"c\">http://x.y</a></p>",
        ),
    ] {
        let converted = carve::djot_to_carve(source);
        assert_eq!(
            carve::to_html(&converted).trim(),
            expected,
            "{source}: {converted}"
        );
    }
}

#[test]
fn pending_attribute_runs_keep_their_containers() {
    let between_tags = regex::Regex::new(r">\s+<").unwrap();
    for (source, expected) in [
        (
            "> {.a}\n> {.b}\n>\n> para",
            "<blockquote><p class=\"a b\">para</p></blockquote>",
        ),
        (
            "> {.a}\n> {.b}\n\npara",
            "<blockquote></blockquote><p class=\"a b\">para</p>",
        ),
        ("{.a}\n> {.b}\n\n", "<blockquote class=\"a\"></blockquote>"),
        (
            "{.a}\n> {.b}\n>\n> para",
            "<blockquote class=\"a\"><p class=\"b\">para</p></blockquote>",
        ),
        (
            "```\n{.a}\n{.b}\n\npara\n```",
            "<pre><code>{.a}\n{.b}\n\npara\n</code></pre>",
        ),
    ] {
        let converted = carve::djot_to_carve(source);
        let html = carve::to_html(&converted);
        assert_eq!(
            between_tags.replace_all(html.trim(), "><"),
            expected,
            "{source}: {converted}"
        );
    }
}

#[test]
fn root_attribute_boundaries_match_native_djot() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/djot_root_attribute_boundaries.json")).unwrap();
    let between_tags = regex::Regex::new(r">\s+<").unwrap();
    let li_open = regex::Regex::new(r"(<(?:li|p)(?: [^>]*)?>)[ \t\r\n]+").unwrap();
    let li_close = regex::Regex::new(r"[ \t\r\n]+(</(?:li|p)>)").unwrap();
    let table_groups = regex::Regex::new(r"</?(?:thead|tbody)>").unwrap();
    let nested_list_indent = regex::Regex::new(r"\n[ \t]+(<(?:ul|ol)(?:\s|>))").unwrap();
    let incidental_attributes =
        regex::Regex::new(r#" (?:aria-label|data-delim|data-task-state)="[^"]*""#).unwrap();
    let input_spacing = regex::Regex::new(r"(<input[^>]*?)/?>[ \t\r\n]*").unwrap();
    let input_attributes = regex::Regex::new(r#"<input[^>]*type="checkbox"[^>]*>"#).unwrap();
    let nested_block_indent =
        regex::Regex::new(r"\n[ \t]+(<(?:blockquote|table)(?:\s|>))").unwrap();
    let html_tags = regex::Regex::new(r"<[^>]+>").unwrap();
    let text_spaces = regex::Regex::new(r"[ \t\r\n]+").unwrap();
    let opening_tag = regex::Regex::new(r"^<([A-Za-z][A-Za-z0-9-]*)([^>]*)>$").unwrap();
    let tag_attribute = regex::Regex::new(r#"[^\s=/>]+(?:=(?:"[^"]*"|'[^']*'|[^\s>]+))?"#).unwrap();
    let canonical_code = |text: &str| {
        use html5ever::tendril::TendrilSink;
        use markup5ever_rcdom::{NodeData, RcDom};
        let dom = html5ever::parse_document(RcDom::default(), Default::default())
            .one(format!("<pre><code>{text}</code></pre>"));
        let mut stack = vec![dom.document];
        let mut value = String::new();
        while let Some(node) = stack.pop() {
            if let NodeData::Text { contents } = &node.data {
                for c in contents.borrow().chars() {
                    match c {
                        '&' => value.push_str("&amp;"),
                        '<' => value.push_str("&lt;"),
                        '>' => value.push_str("&gt;"),
                        '"' => value.push_str("&quot;"),
                        '\'' => value.push_str("&apos;"),
                        c => value.push(c),
                    }
                }
            }
            stack.extend(node.children.borrow().iter().rev().cloned());
        }
        value
    };
    let normalize = |html: &str| {
        let mut code_prefix = "DJOTCODETEXT".to_owned();
        while html.contains(&code_prefix) {
            code_prefix.push('X');
        }
        let mut code_text = Vec::new();
        let mut spaced = String::new();
        let mut end = 0;
        let mut in_code = false;
        let mut in_backlink = false;
        for tag in html_tags.find_iter(html) {
            let text = &html[end..tag.start()];
            if in_code {
                let token = format!("{code_prefix}{}Z", code_text.len());
                code_text.push((token.clone(), canonical_code(text)));
                spaced.push_str(&token);
            } else {
                let text = if in_backlink {
                    text.replace('\u{FE0E}', "")
                } else {
                    text.to_owned()
                };
                spaced.push_str(&text_spaces.replace_all(&text, " "));
            }
            if let Some(parts) = opening_tag.captures(tag.as_str()) {
                let mut attributes: Vec<_> = tag_attribute
                    .find_iter(&parts[2])
                    .map(|attribute| attribute.as_str())
                    .collect();
                attributes.sort_unstable();
                spaced.push('<');
                spaced.push_str(&parts[1]);
                for attribute in attributes {
                    spaced.push(' ');
                    spaced.push_str(attribute);
                }
                spaced.push('>');
            } else {
                spaced.push_str(tag.as_str());
            }
            if tag.as_str().starts_with("<a ") && tag.as_str().contains("role=\"doc-backlink\"") {
                in_backlink = true;
            } else if tag.as_str() == "</a>" {
                in_backlink = false;
            }
            if tag.as_str().starts_with("<code") {
                in_code = true;
            } else if tag.as_str() == "</code>" {
                in_code = false;
            }
            end = tag.end();
        }
        if in_code {
            let token = format!("{code_prefix}{}Z", code_text.len());
            code_text.push((token.clone(), canonical_code(&html[end..])));
            spaced.push_str(&token);
        } else {
            spaced.push_str(&text_spaces.replace_all(&html[end..], " "));
        }
        let html = incidental_attributes.replace_all(spaced.trim(), "");
        let html = input_spacing.replace_all(&html, "$1>");
        let html = input_attributes.replace_all(&html, "<input type=\"checkbox\">");
        let html = nested_block_indent.replace_all(&html, "\n$1");
        let html = between_tags.replace_all(&html, "><");
        let html = table_groups.replace_all(&html, "");
        let html = html.replace(" scope=\"col\"", "");
        let html = li_open.replace_all(&html, "$1");
        let html = li_close.replace_all(&html, "$1");
        let html = nested_list_indent.replace_all(&html, "\n$1").into_owned();
        let tokens =
            regex::Regex::new(&format!("{}([0-9]+)Z", regex::escape(&code_prefix))).unwrap();
        tokens
            .replace_all(&html, |matched: &regex::Captures<'_>| {
                matched[1]
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| code_text.get(index))
                    .map_or_else(|| matched[0].to_owned(), |(_, text)| text.clone())
            })
            .into_owned()
    };
    let mut failures = Vec::new();
    for case in cases.as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let expected = case["html"].as_str().unwrap();
        let converted = carve::djot_to_carve(source);
        let html = carve::to_html(&converted);
        assert_eq!(
            normalize(&carve::to_html(
                &carve::render_carve(&carve::parse(&converted)).unwrap()
            )),
            normalize(&html),
            "{source}: formatting changed the document"
        );
        if let Some(native) = case.get("nativeAst") {
            let document = carve::parse(&converted);
            let carve::ast::BlockNode::List(list) = &document.children[0] else {
                panic!("{source}: expected task list")
            };
            let items = native["children"][0]["children"].as_array().unwrap();
            assert_eq!(list.items.len(), items.len());
            if native["children"][0]["children"][0]["children"][0]["tag"] == "block_quote" {
                assert_eq!(list.items.len(), 1);
                let carve::ast::BlockNode::BlockQuote(quote) = &list.items[0].children[0] else {
                    panic!("{source}: expected quote")
                };
                let paragraphs = native["children"][0]["children"][0]["children"][0]["children"]
                    .as_array()
                    .unwrap();
                assert_eq!(quote.children.len(), paragraphs.len());
                for (paragraph, original) in quote.children.iter().zip(paragraphs) {
                    let carve::ast::BlockNode::Paragraph(paragraph) = paragraph else {
                        panic!("{source}: expected paragraph")
                    };
                    assert_eq!(
                        paragraph
                            .attrs
                            .as_ref()
                            .map(|attrs| attrs.classes.join(" "))
                            .unwrap_or_default(),
                        original["attributes"]["class"].as_str().unwrap_or("")
                    );
                }
                assert!(html.contains("<p class=\"x\">para</p>"), "{html}");
                continue;
            }
            for (item, original) in list.items.iter().zip(items) {
                assert_eq!(item.checked, Some(original["checkbox"] == "checked"));
                let classes = original["attributes"]["class"].as_str().unwrap_or("");
                assert_eq!(
                    item.attrs
                        .as_ref()
                        .map(|attrs| attrs.classes.join(" "))
                        .unwrap_or_default(),
                    classes
                );
            }
            assert_eq!(html.matches("type=\"checkbox\"").count(), items.len());
            if source.contains("{.a}") {
                assert!(html.contains("class=\"a\""));
            }
            if source.contains("_c_") {
                assert!(html.contains("<pre><code>_c_\n</code></pre>"));
            }
            assert_eq!(
                carve::to_html(&carve::render_carve(&document).unwrap()),
                html
            );
            continue;
        }
        if normalize(&html) != normalize(expected) {
            failures.push(format!(
                "{source:?}: {converted:?}\nactual: {}\nexpected: {}",
                normalize(&html),
                normalize(expected)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn a_comment_between_items_adds_no_empty_attribute() {
    let source = "- x\n\n  {% c %}\n\n- y";
    let converted = carve::djot_to_carve(source);
    assert!(!converted.contains("-{}"), "{converted}");
    assert_eq!(carve::to_html(&converted), carve::to_html("- x\n\n- y"));
}

#[test]
fn pending_item_attributes_return_when_a_footnote_closes() {
    let converted = carve::djot_to_carve("[^n]: - x\n\n        {.a}\n\np[^n]");
    let html = carve::to_html(&converted);
    let html = regex::Regex::new(r">\s+<")
        .unwrap()
        .replace_all(&html, "><");
    assert!(!converted.contains('\0'), "{converted:?}");
    assert!(html.contains("<p class=\"a\">p<a"), "{html}");
    assert!(html.contains("<ul><li><p>x</p></li></ul>"), "{html}");
}

#[test]
fn a_referenced_footnote_head_keeps_one_task_list() {
    let converted = carve::djot_to_carve("[^n]: - [ ] a\n      - [ ] b\n\nx[^n]");
    let html = carve::to_html(&converted);
    assert!(!converted.contains('\0'), "{converted:?}");
    assert_eq!(
        regex::Regex::new(r"<ul(?: |>)")
            .unwrap()
            .find_iter(&html)
            .count(),
        1
    );
    assert_eq!(html.matches("type=\"checkbox\"").count(), 2);
}

#[test]
fn nested_footnote_item_attributes_return_before_the_parent_fence() {
    let converted = carve::djot_to_carve("[^a]: - x\n\n      [^b]: - y\n\n              {.c}\n\n      ```\n      z\n      ```\n\np[^a] q[^b]");
    let html = carve::to_html(&converted);
    assert!(!converted.contains('\0'), "{converted:?}");
    assert!(
        html.contains("<pre class=\"c\"><code>z\n</code></pre>"),
        "{html}"
    );
    assert!(!html.contains("<p class=\"c\">"), "{html}");
}

#[test]
fn a_footnote_paragraph_keeps_one_literal_quote_marker() {
    let converted = carve::djot_to_carve("[^n]: > a\n      > >\n\nx[^n]");
    let html = carve::to_html(&converted);
    let html = regex::Regex::new(r">\s+<")
        .unwrap()
        .replace_all(&html, "><");
    assert!(
        html.contains("<blockquote><p>a\n&gt;</p></blockquote>"),
        "{html}"
    );
}
