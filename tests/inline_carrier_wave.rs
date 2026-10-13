use carve::{migrate_markdown, to_markdown_with_options, Options};
use serde_json::Value;

#[test]
fn inline_markers_match_reference_bytes_in_their_eligible_slots() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/inline_carrier_wave.json")).unwrap();
    let mut failures = Vec::new();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let exported = to_markdown_with_options(source, &Options::default().with_roundtrip(true));
        if exported != row["exported"].as_str().unwrap() {
            failures.push(format!(
                "{source:?}: expected {:?}, actual {exported:?}",
                row["exported"]
            ));
        } else {
            let imported = migrate_markdown(&exported);
            if carve::to_html(&imported.value) != carve::to_html(row["imported"].as_str().unwrap())
            {
                failures.push(format!(
                    "{source:?}: expected {:?}, imported {:?}",
                    row["imported"], imported.value
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn invalid_or_ineligible_inline_markers_match_reference_recovery() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/inline_carrier_negative_wave.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let imported = migrate_markdown(source);
        assert_eq!(
            carve::to_html(&imported.value),
            carve::to_html(row["imported"].as_str().unwrap()),
            "{source}: {}",
            imported.value
        );
        let codes: Vec<_> = imported
            .report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .filter(|code| code.starts_with("carrier-"))
            .collect();
        let expected: Vec<_> = row["codes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|code| code.as_str().unwrap())
            .collect();
        assert_eq!(codes, expected, "{source}");
    }
}

#[test]
fn repeated_inline_marker_losses_keep_each_authored_line() {
    let imported = migrate_markdown("a<!-- carve: {.x} -->b\n\nc<!-- carve: {.x} -->d\n");
    let paths: Vec<_> = imported
        .report
        .diagnostics
        .iter()
        .filter(|row| row.code == "carrier-markers-damaged")
        .map(|row| row.path.as_deref())
        .collect();
    assert_eq!(paths, [Some("line:1"), Some("line:3")]);
}
