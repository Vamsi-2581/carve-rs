//! A closer's indent is measured in source columns (carve#3141): a tab-written
//! closer at a raised fence's column closes it like the same closer in spaces.

const EXPECTED: &str =
    "<ul>\n  <li>a\n    <pre><code>code\n</code></pre>\n    tail\nafter\n  </li>\n</ul>";

#[test]
fn a_tab_closer_at_a_space_opener_closes_the_fence() {
    let source = "- a\n        ```\n    code\n\t\t```\n    tail\nafter\n";
    assert_eq!(carve::to_html(source), EXPECTED);
    assert_eq!(
        carve::to_html_with_options(source, &carve::Options::new()),
        EXPECTED
    );
}

#[test]
fn the_same_closer_in_spaces_closes_it_too() {
    let source = "- a\n        ```\n    code\n        ```\n    tail\nafter\n";
    assert_eq!(carve::to_html(source), EXPECTED);
}
