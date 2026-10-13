//! PART 9 §13 T5: a span never covers another cell (markup-carve/carve#3047).

fn compact(html: &str) -> String {
    html.lines().map(str::trim).collect()
}

fn body(source: &str) -> String {
    let html = compact(&carve::to_html(source));
    let start = html.find("<tbody>").expect("a body") + "<tbody>".len();
    let end = html.find("</tbody>").expect("a body");
    html[start..end].to_string()
}

fn attr(tag: &str, name: &str) -> usize {
    let key = format!("{name}=\"");
    tag.find(&key).map_or(1, |at| {
        let rest = &tag[at + key.len()..];
        rest[..rest.find('"').unwrap()].parse().unwrap()
    })
}

/// Places every cell of every row group on the HTML table grid and reports a
/// slot taken twice or a row wider than the first.
fn grid_errors(html: &str) -> Vec<String> {
    let html = compact(html);
    let mut errors = Vec::new();
    let mut width: Option<usize> = None;
    for group in ["thead", "tbody", "tfoot"] {
        let open = format!("<{group}>");
        let Some(start) = html.find(&open) else {
            continue;
        };
        let end = html.find(&format!("</{group}>")).unwrap();
        let rows: Vec<&str> = html[start + open.len()..end]
            .split("<tr>")
            .skip(1)
            .map(|row| row.trim_end_matches("</tr>"))
            .collect();
        let mut taken = std::collections::BTreeSet::new();
        let mut row_width = vec![0usize; rows.len() + 8];
        for (r, row) in rows.iter().enumerate() {
            let mut col = 0;
            for piece in row.split('<').skip(1) {
                if !(piece.starts_with("td") || piece.starts_with("th")) {
                    continue;
                }
                let tag = &piece[..piece.find('>').unwrap()];
                let (rowspan, colspan) = (attr(tag, "rowspan"), attr(tag, "colspan"));
                while taken.contains(&(r, col)) {
                    col += 1;
                }
                for dr in 0..rowspan {
                    for dc in 0..colspan {
                        if !taken.insert((r + dr, col + dc)) {
                            errors.push(format!("overlap at {}:{}", r + dr, col + dc));
                        }
                        row_width[r + dr] = row_width[r + dr].max(col + dc + 1);
                    }
                }
                col += colspan;
            }
        }
        for &w in row_width.iter().filter(|&&w| w > 0) {
            match width {
                None => width = Some(w),
                Some(first) if w > first => errors.push(format!("row {w} wide, table {first}")),
                _ => {}
            }
        }
    }
    errors
}

#[test]
fn a_caret_under_a_two_column_cell_takes_the_marker_beside_it() {
    let html = compact(&carve::to_html(
        "|= 2-col header|<|= header|\n|spans 3 rows|foo|bar|\n|^|spans 4 cells|<|\n|^|^|<|\n",
    ));
    assert!(html.contains(
        r#"<thead><tr><th scope="col" colspan="2">2-col header</th><th scope="col">header</th></tr></thead>"#
    ), "{html}");
    assert!(html.contains(
        r#"<tbody><tr><td rowspan="3">spans 3 rows</td><td>foo</td><td>bar</td></tr><tr><td rowspan="2" colspan="2">spans 4 cells</td></tr><tr></tr></tbody>"#
    ), "{html}");
}

#[test]
fn a_caret_whose_row_strip_holds_a_cell_is_blocked() {
    assert_eq!(
        body("| a | < |\n| ^ | b |\n"),
        r#"<tr><td colspan="2">a</td></tr><tr><td></td><td>b</td></tr>"#
    );
}

#[test]
fn a_colspan_beside_a_cell_from_an_earlier_row_is_blocked() {
    assert_eq!(
        body("| p | q | r | s |\n|---|---|---|---|\n| a | b | c | d |\n| p | ^ | < | e |\n"),
        r#"<tr><td>a</td><td rowspan="2">b</td><td>c</td><td>d</td></tr><tr><td>p</td><td></td><td>e</td></tr>"#
    );
}

#[test]
fn every_three_by_three_table_of_cells_and_markers_is_a_valid_grid() {
    let alphabet = ["x", "^", "<"];
    for n in 0..3usize.pow(9) {
        let cells: Vec<&str> = (0..9).map(|k| alphabet[n / 3usize.pow(k) % 3]).collect();
        let source: String = cells
            .chunks(3)
            .map(|row| format!("| {} |\n", row.join(" | ")))
            .collect();
        let errors = grid_errors(&carve::to_html(&source));
        assert!(errors.is_empty(), "{source}{errors:?}");
    }
}
