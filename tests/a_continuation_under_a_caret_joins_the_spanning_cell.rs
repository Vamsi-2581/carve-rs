//! PART 5 T6: a continuation row's text under a `^` cell joins the cell that
//! owns the rowspan.

fn compact(source: &str) -> String {
    carve::to_html(source).lines().map(str::trim).collect()
}

#[test]
fn a_continuation_after_the_caret_row_joins_the_spanning_cell() {
    assert_eq!(
        compact("| a | b |\n| ^ | c |\n+ more | d |\n"),
        "<table><tbody><tr><td rowspan=\"2\">a more</td><td>b</td></tr><tr><td>c d</td></tr></tbody></table>"
    );
    assert_eq!(
        compact("| a | b |\n| ^ | c |\n| ^ | e |\n+ more | d |\n"),
        "<table><tbody><tr><td rowspan=\"3\">a more</td><td>b</td></tr><tr><td>c</td></tr><tr><td>e d</td></tr></tbody></table>"
    );
    assert!(compact("| A |\n|---|\n| ^ |\n+ x |\n").contains("rowspan=\"2\">A x</th>"));
}

#[test]
fn a_continuation_before_the_caret_row_joins_its_own_row() {
    assert_eq!(
        compact("| a | b |\n+ more | d |\n| ^ | c |\n"),
        "<table><tbody><tr><td rowspan=\"2\">a more</td><td>b d</td></tr><tr><td>c</td></tr></tbody></table>"
    );
}

#[test]
fn positions_keep_the_joined_text() {
    let source = "| a | b |\n| ^ | c |\n+ more | d |\n";
    let html = carve::to_html_with_options(source, &carve::Options::default().with_positions(true));
    assert!(html.contains("a more"), "{html}");
}
