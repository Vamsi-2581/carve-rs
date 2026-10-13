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
fn depth_on_container_opener_drops_pair() {
    let markdown = "<!-- carve:2 ::: note -->\nBody.\n<!-- carve: ::: -->\n\n<!-- carve: ::: tip -->\nok\n<!-- carve: ::: -->\n";
    let imported = migrate_markdown(markdown);
    let found = nodes(&imported.value);
    assert_eq!(count(markdown, "carrier-marker-dropped"), 1);
    assert_eq!(count(markdown, "carrier-markers-damaged"), 0);
    assert!(found
        .iter()
        .any(|node| node["type"] == "admonition" && node["kind"] == "tip"));
    assert!(!found
        .iter()
        .any(|node| node["type"] == "admonition" && node["kind"] == "note"));
    assert!(found
        .iter()
        .any(|node| node["type"] == "paragraph" && node["children"][0]["value"] == "Body."));
}

#[test]
fn depth_on_container_closer_drops_pair() {
    let markdown = "<!-- carve: ::: note -->\nBody.\n<!-- carve:2 ::: -->\n\n<!-- carve: ::: tip -->\nok\n<!-- carve: ::: -->\n";
    let imported = migrate_markdown(markdown);
    let found = nodes(&imported.value);
    assert_eq!(count(markdown, "carrier-marker-dropped"), 1);
    assert_eq!(count(markdown, "carrier-markers-damaged"), 0);
    assert!(found
        .iter()
        .any(|node| node["type"] == "admonition" && node["kind"] == "tip"));
    assert!(!found
        .iter()
        .any(|node| node["type"] == "admonition" && node["kind"] == "note"));
    assert!(found
        .iter()
        .any(|node| node["type"] == "paragraph" && node["children"][0]["value"] == "Body."));
}

#[test]
fn depth_on_attribute_marker_drops_alone() {
    let markdown =
        "<!-- carve:2 {.x} -->\n# Head\n\n<!-- carve: ::: tip -->\nok\n<!-- carve: ::: -->\n";
    let imported = migrate_markdown(markdown);
    let found = nodes(&imported.value);
    assert_eq!(count(markdown, "carrier-marker-dropped"), 1);
    assert_eq!(count(markdown, "carrier-markers-damaged"), 0);
    assert!(found
        .iter()
        .any(|node| node["type"] == "heading" && node["attrs"]["classes"].is_null()));
    assert!(found
        .iter()
        .any(|node| node["type"] == "admonition" && node["kind"] == "tip"));
}

#[test]
fn nested_definition_depth_remains_valid() {
    let markdown = "<!-- carve: :: Outer -->\n**Outer**\n\n<!-- carve: : -->\nbody\n\n<!-- carve:2 :: Inner -->\n**Inner**\n\n<!-- carve:2 : -->\ninner body\n\n<!-- carve:2 -->\n\n<!-- carve: -->\n";
    let imported = migrate_markdown(markdown);
    assert_eq!(count(markdown, "carrier-marker-dropped"), 0);
    assert_eq!(count(markdown, "carrier-markers-damaged"), 0);
    assert!(nodes(&imported.value)
        .iter()
        .any(|node| node["type"] == "definition_list"));
}
