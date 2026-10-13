use carve::{migrate_markdown, parse, to_json};
use serde_json::Value;

fn nodes(source: &str) -> Vec<Value> {
    fn visit(value: &Value, found: &mut Vec<Value>) {
        match value {
            Value::Object(map) => {
                if value["type"].is_string() {
                    found.push(value.clone());
                }
                for child in map.values() {
                    visit(child, found);
                }
            }
            Value::Array(items) => {
                for item in items {
                    visit(item, found);
                }
            }
            _ => {}
        }
    }
    let tree: Value = serde_json::from_str(&to_json(&parse(source))).unwrap();
    let mut found = Vec::new();
    visit(&tree, &mut found);
    found
}

fn count(source: &str, code: &str) -> usize {
    migrate_markdown(source)
        .report
        .diagnostics
        .iter()
        .filter(|row| row.code == code)
        .count()
}

#[test]
fn invalid_figure_pair_takes_caption_marker() {
    let markdown = "<!-- carve: ::: {key=\"u} figure -->\n<!-- carve: :::: panel -->\n![a](x.png)\n\n<!-- carve: :::: -->\n<!-- carve: ::: -->\n<!-- carve: ^ Group caption -->\n**Group caption**\n\ntext\n";
    let imported = migrate_markdown(markdown);
    let found = nodes(&imported.value);
    assert_eq!(count(markdown, "carrier-marker-dropped"), 1);
    assert_eq!(count(markdown, "carrier-markers-damaged"), 0);
    assert!(found
        .iter()
        .any(|node| node["type"] == "admonition" && node["kind"] == "panel"));
    assert!(!found.iter().any(|node| node["type"] == "figure_group"));
    assert!(imported.value.contains("Group caption"));
}

#[test]
fn invalid_figure_closer_takes_caption_marker() {
    let markdown = "<!-- carve: ::: figure -->\n<!-- carve: :::: panel -->\n![a](x.png)\n\n<!-- carve: :::: -->\n<!-- carve:2 ::: -->\n<!-- carve: ^ Group caption -->\n**Group caption**\n\ntext\n";
    let imported = migrate_markdown(markdown);
    let found = nodes(&imported.value);
    assert_eq!(count(markdown, "carrier-marker-dropped"), 1);
    assert_eq!(count(markdown, "carrier-markers-damaged"), 0);
    assert!(found
        .iter()
        .any(|node| node["type"] == "admonition" && node["kind"] == "panel"));
    assert!(!found.iter().any(|node| node["type"] == "figure_group"));
    assert!(imported.value.contains("Group caption"));
}

#[test]
fn valid_figure_pair_keeps_caption_marker() {
    let markdown = "<!-- carve: ::: figure -->\n<!-- carve: :::: panel -->\n![a](x.png)\n\n<!-- carve: :::: -->\n<!-- carve: ::: -->\n<!-- carve: ^ Group caption -->\n**Group caption**\n\ntext\n";
    let imported = migrate_markdown(markdown);
    assert_eq!(count(markdown, "carrier-marker-dropped"), 0);
    assert_eq!(count(markdown, "carrier-markers-damaged"), 0);
    assert!(nodes(&imported.value)
        .iter()
        .any(|node| node["type"] == "figure_group"));
}
