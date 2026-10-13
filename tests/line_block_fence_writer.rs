use carve::{to_carve, to_html};

#[test]
fn paired_payload_runs_hide_the_line_block_closer_after_formatting() {
    for source in [
        "> - ::: |\n>     ```\n>   :::\n>     ```\n>   :::\n>   :::bad\n>   :::\n- tail\n",
        "- ::: |\n    ```\n  :::\n    ```\n  :::\n  para\ntail\n",
        "- ::: |\n   ```\n  :::\n    ```\n  :::\n  para\ntail\n",
    ] {
        let formatted = to_carve(source);
        assert_eq!(
            to_html(&formatted),
            to_html(source),
            "{source}: {formatted}"
        );
        assert_eq!(to_carve(&formatted), formatted);
    }
}

#[test]
fn a_prefixed_or_attributed_span_cannot_hide_the_line_block_closer() {
    for node in [
        serde_json::json!({"type":"code", "value":"\n:::\n", "attrs":{"classes":["x"]}}),
        serde_json::json!({"type":"math", "display":false, "content":"\n:::\n"}),
        serde_json::json!({"type":"literal_inline", "content":"\n:::\n"}),
        serde_json::json!({"type":"raw_inline", "format":"html", "content":"\n:::\n"}),
    ] {
        let value = serde_json::json!({"type":"document", "srcByteLength":0,
            "children":[{"type":"line_block", "children":[{"type":"paragraph", "children":[node]}]}]});
        let doc = carve::from_json(&value.to_string()).unwrap();
        let error = carve::render_carve(&doc).unwrap_err();
        let carve::RenderCarveError::SourceUnspellable(error) = error else {
            panic!("expected a source refusal");
        };
        assert!(error
            .to_string()
            .contains("cannot hide a line-block closer"));
    }
}
