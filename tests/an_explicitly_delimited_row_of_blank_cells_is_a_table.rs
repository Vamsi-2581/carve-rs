//! A standard row with an opening and a closing pipe and at least one cell slot
//! is a table row, whatever its cells hold (CARVE-P9-082, markup-carve/carve#2879).
//! `||` and `| |` are one empty cell, `|||` and `| | |` two. A lone `|` has no
//! slot and stays prose.

use carve::{to_html, to_html_with_options, Options};

/// The library facade and the position-tracking path must agree - the #908
/// guard.
fn flat(source: &str) -> String {
    let facade = to_html(source);
    let positions = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(
        facade, positions,
        "the library path and the position-tracking path disagree on {source:?}"
    );
    facade.split_whitespace().collect::<Vec<_>>().join(" ")
}

const ONE_EMPTY: &str = "<table> <tbody> <tr><td></td></tr> </tbody> </table>";
const TWO_EMPTY: &str = "<table> <tbody> <tr><td></td><td></td></tr> </tbody> </table>";

#[test]
fn a_pipe_pair_is_one_empty_cell() {
    assert_eq!(flat("||\n"), ONE_EMPTY);
    assert_eq!(flat("| |\n"), ONE_EMPTY);
}

#[test]
fn each_further_pipe_adds_a_cell() {
    assert_eq!(flat("|||\n"), TWO_EMPTY);
    assert_eq!(flat("| | |\n"), TWO_EMPTY);
    assert_eq!(
        flat("||||\n"),
        "<table> <tbody> <tr><td></td><td></td><td></td></tr> </tbody> </table>"
    );
}

#[test]
fn a_lone_pipe_and_an_unclosed_row_stay_prose() {
    assert_eq!(flat("|\n"), "<p>|</p>");
    assert_eq!(flat("| a | b\n"), "<p>| a | b</p>");
    assert_eq!(flat("\\| |\n"), "<p>| |</p>");
}

#[test]
fn a_blank_row_interrupts_a_paragraph() {
    assert_eq!(flat("text\n| |\n"), format!("<p>text</p> {ONE_EMPTY}"));
}

#[test]
fn a_tab_between_the_pipes_is_content() {
    assert_eq!(
        to_html("|\t|\n"),
        "<table>\n  <tbody>\n    <tr><td>\t</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn an_empty_marked_cell_keeps_its_marker() {
    assert_eq!(
        flat("|= |\n"),
        "<table> <thead> <tr><th scope=\"col\"></th></tr> </thead> </table>"
    );
    assert_eq!(
        flat("|> |\n"),
        "<table> <tbody> <tr><td style=\"text-align: right;\"></td></tr> </tbody> </table>"
    );
    assert_eq!(
        flat("|{#x} |\n"),
        "<table> <tbody> <tr><td id=\"x\"></td></tr> </tbody> </table>"
    );
}

#[test]
fn a_marker_with_no_separator_space_is_content() {
    assert_eq!(
        flat("|=|\n"),
        "<table> <tbody> <tr><td>=</td></tr> </tbody> </table>"
    );
    assert_eq!(
        flat("|>|\n"),
        "<table> <tbody> <tr><td>&gt;</td></tr> </tbody> </table>"
    );
}

#[test]
fn a_blank_row_inside_a_table_stays_in_it() {
    assert_eq!(
        flat("|a|b|\n|||\n|c|d|\n"),
        "<table> <tbody> <tr><td>a</td><td>b</td></tr> <tr><td></td><td></td></tr> \
         <tr><td>c</td><td>d</td></tr> </tbody> </table>"
    );
}

#[test]
fn the_writer_spells_a_blank_row_as_a_row() {
    for (source, written) in [
        ("||\n", "| |"),
        ("| | |\n", "| | |"),
        ("|a|b|\n| | |\n", "| a | b |\n| | |"),
    ] {
        let out = carve::to_carve(source);
        assert_eq!(out.trim_end(), written, "{source:?}");
        assert_eq!(to_html(&out), to_html(source), "{source:?}");
    }
}
