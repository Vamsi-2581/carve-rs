#[test]
fn unused_references_survive_each_non_html_target() {
    for source in [
        "[Text][REF]\n\n[ref]: /u\n",
        "[ref]: /u\n",
        "[ref]: /u \"title\"\n",
    ] {
        let doc = carve::parse(source);
        let definition = if source.contains("title") {
            "[ref]: /u \"title\""
        } else {
            "[ref]: /u"
        };
        assert!(carve::render_markdown(&doc).unwrap().contains(definition));
        assert!(carve::render_plain_text(&doc).unwrap().contains(definition));
        assert!(carve::render_ansi(&doc).unwrap().contains(definition));
        assert!(!carve::render_html(&doc).unwrap().contains("/u"));
    }
    let doc = carve::parse("[t][r]\n\n[r]: /u\n");
    assert_eq!(carve::render_markdown(&doc).unwrap(), "[t](/u)\n");
    assert_eq!(carve::render_plain_text(&doc).unwrap(), "t\n");
    assert!(!carve::render_ansi(&doc).unwrap().contains("[r]:"));
}

#[test]
fn unused_reference_rows_match_the_clause_and_non_html_targets() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/wave7_unused_references.json")).unwrap();
    let ansi = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    for row in rows.as_array().unwrap() {
        let doc = carve::parse(row["source"].as_str().unwrap());
        for (target, actual) in [
            ("markdown", carve::render_markdown(&doc).unwrap()),
            ("plain", carve::render_plain_text(&doc).unwrap()),
            ("ansi", carve::render_ansi(&doc).unwrap()),
        ] {
            let definitions = |text: &str| {
                let text = ansi.replace_all(text, "");
                let mut lines: Vec<String> = text
                    .lines()
                    .filter(|line| line.contains("]: "))
                    .map(|line| line.trim().to_string())
                    .collect();
                lines.sort();
                lines
            };
            assert_eq!(
                definitions(&actual),
                row["definitions"][target]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|line| line.as_str().unwrap().to_string())
                    .collect::<Vec<_>>(),
                "{} {target}",
                row["id"]
            );
        }
    }
}
