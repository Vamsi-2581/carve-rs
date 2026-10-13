//! Closed nested fences end before a following description entry.
//! Unfinished nested fences release flush-left lines as document content.

use carve::{to_carve, to_html};

fn flat(source: &str) -> String {
    to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

const TWO_ENTRIES: &str = "<dl> <dt>t</dt> <dd> <ul> <li> \
     <pre><code class=\"language-x\">code </code></pre> </li> </ul> </dd> \
     <dt>t2</dt> <dd>plain</dd> </dl>";

#[test]
fn the_blank_less_closed_fence_form_reads_as_two_entries() {
    // The re-emitted form: the fence is CLOSED by an indented closer, and there
    // is no separating blank before the second term. This was the sole engine
    // that folded `:: t2` / `: plain` into the first entry.
    assert_eq!(
        flat(":: t\n: - ```x\n    code\n    ```\n:: t2\n: plain\n"),
        TWO_ENTRIES
    );
}

#[test]
fn the_source_unterminated_form_reads_as_two_entries_too() {
    assert_eq!(
        flat(":: t\n: - ``` x\ncode\n\n:: t2\n: plain\n"),
        "<dl> <dt>t</dt> <dd> <ul> <li> <pre><code class=\"language-x\"></code></pre> </li> </ul> </dd> </dl> <p>code</p> <dl> <dt>t2</dt> <dd>plain</dd> </dl>"
    );
}

#[test]
fn the_writer_re_emits_the_blank_less_form_and_it_round_trips() {
    let source = ":: t\n: - ``` x\ncode\n\n:: t2\n: plain\n";
    let formatted = to_carve(source);
    assert_eq!(
        formatted,
        ":: t\n: - ```x\n    ```\n\ncode\n\n:: t2\n: plain\n"
    );
    assert_eq!(flat(&formatted), flat(source));
}
