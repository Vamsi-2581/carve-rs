use super::*;
use std::collections::HashMap;

fn starts_quote(text: &str) -> bool {
    text.starts_with('>')
        && text
            .as_bytes()
            .get(1)
            .map_or(true, |b| matches!(b, b' ' | b'\t'))
}

fn thematic(text: &str) -> bool {
    let bytes: Vec<_> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    bytes.len() >= 3 && bytes.iter().all(|b| matches!(b, b'*' | b'-'))
}

fn separator(text: &str) -> bool {
    text.len() >= 2
        && text.starts_with('|')
        && text.ends_with('|')
        && text[1..text.len() - 1].split('|').all(|cell| {
            let cell = cell.trim();
            cell.contains('-') && cell.bytes().all(|b| matches!(b, b'-' | b':'))
        })
}

#[derive(Clone)]
struct List {
    source: usize,
    column: usize,
    target: usize,
    target_column: usize,
    written: u8,
    kind: String,
    start: usize,
    lazy: bool,
    loose: bool,
}

/// Djot attaches an attribute line with no block after it to nothing, so drop it.
fn drop_orphan_attribute_line(result: &mut Vec<String>) {
    let Some(last) = result.last() else { return };
    let last = last.trim_end();
    let at = last.find(|c: char| !c.is_whitespace());
    let Some(at) = at else { return };
    if !last[at..].starts_with('{') {
        return;
    }
    if read_djot_word_attributes(last, at).is_some_and(|(end, _)| end == last.len()) {
        result.pop();
    }
}

fn compact_djot_list_prefix(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains('0') && !text.contains('(') {
        return std::borrow::Cow::Borrowed(text);
    }
    let marker = cached_regex!(r"^(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
    let quote = cached_regex!(r"^>(?:[ \t]|$)").unwrap();
    let task = cached_regex!(r"^\[[ xX]\][ \t]+").unwrap();
    let mut result = String::with_capacity(text.len());
    let mut at = 0;
    let mut changed = false;
    while at < text.len() {
        let indent = text[at..]
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        result.push_str(&text[at..at + indent]);
        at += indent;
        if let Some(prefix) = quote.find(&text[at..]) {
            result.push_str(prefix.as_str());
            at += prefix.end();
            continue;
        }
        let Some(item) = marker.find(&text[at..]) else {
            break;
        };
        let prefix = item.as_str();
        let start = usize::from(prefix.starts_with('('));
        let mut end = start;
        while prefix.as_bytes().get(end) == Some(&b'0')
            && prefix
                .as_bytes()
                .get(end + 1)
                .is_some_and(u8::is_ascii_digit)
        {
            end += 1;
        }
        changed |= end > start;
        if prefix.starts_with('(') {
            let close = prefix.find(')').unwrap();
            result.push_str(&prefix[end..close]);
            result.push('.');
            result.push_str(&prefix[close + 1..]);
            changed = true;
        } else {
            result.push_str(&prefix[end..]);
        }
        at += item.end();
        if matches!(prefix.as_bytes()[0], b'-' | b'*' | b'+') {
            if let Some(prefix) = task.find(&text[at..]) {
                result.push_str(prefix.as_str());
                at += prefix.end();
            }
        }
    }
    result.push_str(&text[at..]);
    if changed {
        std::borrow::Cow::Owned(result)
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

fn quoted_list_boundaries<'a>(
    source: &'a str,
    mask: &str,
    fence_mask: &str,
    rows: &[bool],
    slot_prefix: Option<&str>,
    has_definition_syntax: bool,
) -> std::borrow::Cow<'a, str> {
    if !source.contains('>') || !source.contains('\n') {
        return std::borrow::Cow::Borrowed(source);
    }
    let quote = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*").unwrap();
    let marker = cached_regex!(r"^((?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+)(\[[ xX]\][ \t]+)?([^ \t])").unwrap();
    let embedded = cached_regex!(r"^(?:>(?:[ \t]+|$))+").unwrap();
    let heading = cached_regex!(r"^#{1,6}(?:[ \t]|$)").unwrap();
    let mut insertions = Vec::new();
    let mut div_closers = HashMap::new();
    if source.contains(":::") {
        djot_inline_boundaries_with_div_closers(source, mask, true, Some(&mut div_closers));
    }
    let reference = cached_regex!(r"^\[[^\]\n]*\]:").unwrap();
    let mut offset = 0;
    let mut depth = 0;
    let mut boundary = String::new();
    let mut paragraph = false;
    let mut previous_blank = true;
    let mut lists: Vec<(usize, usize, usize, usize, usize, usize)> = Vec::new();
    let rejected_comment =
        cached_regex!(r"^\\\{\x00DJOTINVALIDATTR\x00[0-9]+\x00%[^%}]*$").unwrap();
    let mut rejected_comment_open = false;
    for (row_index, ((line, masked), fenced)) in source
        .split('\n')
        .zip(mask.split('\n'))
        .zip(fence_mask.split('\n'))
        .enumerate()
    {
        if slot_prefix.is_some_and(|prefix| line.starts_with(prefix)) {
            paragraph = false;
            previous_blank = true;
            rejected_comment_open = false;
            offset += line.len() + 1;
            continue;
        }
        let line_offset = offset;
        let mut item_edit = None;
        let mut item_target = None;
        offset += line.len() + 1;
        if fenced != line {
            rejected_comment_open = false;
            depth = 0;
            paragraph = false;
            previous_blank = true;
            continue;
        }
        let prefix = quote.find(line).unwrap().as_str();
        let pending_comment = rejected_comment_open && paragraph && !previous_blank;
        let opens_rejected_comment = rejected_comment.is_match(&line[djot_content_start(line)..]);
        let comment_continuation = pending_comment
            && !line[prefix.len()..]
                .trim_start_matches([' ', '\t'])
                .starts_with("\\>");
        rejected_comment_open = opens_rejected_comment
            || pending_comment
                && !line[prefix.len()..].trim_matches([' ', '\t']).is_empty()
                && !line.contains(['%', '}']);
        let quote_depth = prefix.bytes().filter(|&b| b == b'>').count();
        let mut columns = Vec::new();
        let mut bases = Vec::new();
        let mut positions = Vec::new();
        let bytes = line.as_bytes();
        let mut at = 0;
        for level in 0..=quote_depth {
            let start = at;
            bases.push(start);
            while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
                at += 1;
            }
            columns.push(at - start);
            if level < quote_depth {
                positions.push(at);
                at += 1;
                if bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
                    at += 1;
                }
            }
        }
        if positions
            .first()
            .is_some_and(|&at| masked.as_bytes().get(at) != Some(&b'>'))
        {
            previous_blank = false;
            continue;
        }
        let remainder = &line[prefix.len()..];
        let raw_indent = remainder.len() - remainder.trim_start_matches([' ', '\t']).len();
        let source_prefix = lists
            .last()
            .filter(|&&(owner_depth, _, _, _, _, _)| owner_depth == quote_depth)
            .map_or(prefix.len(), |&(_, _, _, source_prefix, _, _)| {
                source_prefix
            });
        let indent = (raw_indent + prefix.len()).saturating_sub(source_prefix);
        columns[quote_depth] = indent;
        let column_at = |owner_depth: usize, owner_prefix: usize| {
            if quote_depth > owner_depth {
                (columns[owner_depth] + bases[owner_depth]).saturating_sub(owner_prefix)
            } else {
                indent
            }
        };
        let text = &remainder[raw_indent..];
        let visible = masked.get(prefix.len() + raw_indent..).unwrap_or("");
        if previous_blank && !line.trim_matches([' ', '\t']).is_empty() {
            if depth > quote_depth
                && !paragraph
                && !marker.is_match(visible)
                && lists.last().is_some_and(
                    |&(owner_depth, column, _width, _source_prefix, _, _)| {
                        owner_depth > quote_depth
                            || owner_depth == quote_depth
                                && column_at(owner_depth, _source_prefix) < column + 1
                    },
                )
            {
                insertions.push((line_offset, line_offset, format!("{}\n", prefix.trim_end())));
            }
            while lists.last().is_some_and(
                |&(owner_depth, column, _width, _source_prefix, _, _)| {
                    owner_depth > quote_depth
                        || owner_depth == quote_depth
                            && column_at(owner_depth, _source_prefix) < column + 1
                },
            ) {
                lists.pop();
            }
        }
        if text.is_empty() {
            while lists
                .last()
                .is_some_and(|&(owner_depth, _, _, _, _, _)| owner_depth > quote_depth)
            {
                lists.pop();
            }
        }
        let ends_item =
            lists
                .last()
                .is_some_and(|&(owner_depth, column, _, _source_prefix, _, _)| {
                    owner_depth > quote_depth || owner_depth == quote_depth && indent <= column
                });
        if thematic(visible)
            || heading.is_match(visible)
            || visible.starts_with(":::")
            || visible.starts_with('|')
        {
            while lists
                .last()
                .is_some_and(|&(owner_depth, column, _, _source_prefix, _, _)| {
                    owner_depth > quote_depth || owner_depth == quote_depth && indent <= column
                })
            {
                lists.pop();
            }
        }
        if let Some(index) = lists
            .iter()
            .take_while(|&&(owner_depth, _, _, _, _, _)| owner_depth < quote_depth)
            .position(|&(owner_depth, column, _, owner_prefix, _, _)| {
                owner_depth < quote_depth && column_at(owner_depth, owner_prefix) <= column
            })
        {
            let owner_depth = lists[index].0;
            if paragraph && !previous_blank && !(owner_depth == 0 && positions.first() == Some(&0))
            {
                let end = boundary
                    .match_indices('>')
                    .nth(owner_depth)
                    .map_or(boundary.len(), |(at, _)| at + 1);
                insertions.push((line_offset, line_offset, format!("{}\n", &boundary[..end])));
            }
            lists.truncate(index);
            paragraph = false;
        }
        let outside_item =
            lists
                .last()
                .is_some_and(|&(owner_depth, column, _width, _source_prefix, _, _)| {
                    quote_depth > owner_depth && column_at(owner_depth, _source_prefix) < column + 1
                });
        if quote_depth > depth && paragraph && !outside_item {
            let mut owned = vec![None; depth];
            for &(owner_depth, column, width, _source_prefix, target, _) in lists
                .iter()
                .take_while(|&&(owner_depth, _, _, _, _, _)| owner_depth < depth)
            {
                if owner_depth < depth && column_at(owner_depth, _source_prefix) > column {
                    owned[owner_depth] = Some(target + width);
                }
            }
            let mut normalized = String::new();
            for column in owned {
                normalized.push_str(&" ".repeat(column.unwrap_or(0)));
                normalized.push_str("> ");
            }
            normalized.push('\\');
            normalized.push_str(&prefix[positions[depth]..]);
            insertions.push((line_offset, line_offset + prefix.len(), normalized));
            continue;
        }
        if paragraph
            && !outside_item
            && !ends_item
            && !div_closers.contains_key(&line_offset)
            && quote_depth == depth
            && quote_depth > 0
            && (heading.is_match(visible) || thematic(visible) || visible.starts_with(":::"))
        {
            if !has_definition_syntax {
                let mut owned = vec![None; quote_depth];
                for &(owner_depth, column, width, _source_prefix, target, _) in lists
                    .iter()
                    .take_while(|&&(owner_depth, _, _, _, _, _)| owner_depth < quote_depth)
                {
                    if owner_depth < quote_depth && column_at(owner_depth, _source_prefix) > column
                    {
                        owned[owner_depth] = Some(target + width);
                    }
                }
                let mut normalized = String::new();
                for (level, column) in owned.into_iter().enumerate() {
                    normalized.push_str(&" ".repeat(column.unwrap_or(0)));
                    normalized.push('>');
                    if level + 1 < quote_depth || !prefix.ends_with('>') {
                        normalized.push(' ');
                    }
                }
                if normalized != prefix {
                    insertions.push((line_offset, line_offset + prefix.len(), normalized));
                }
            }
            insertions.push((
                line_offset + prefix.len() + raw_indent,
                line_offset + prefix.len() + raw_indent,
                "\\".to_owned(),
            ));
            continue;
        }
        if outside_item {
            while lists.last().is_some_and(
                |&(owner_depth, column, _width, _source_prefix, _, _)| {
                    quote_depth > owner_depth && column_at(owner_depth, _source_prefix) < column + 1
                },
            ) {
                lists.pop();
            }
            paragraph = false;
        }
        previous_blank =
            line.trim_matches([' ', '\t']).is_empty() || quote_depth > 0 && text.is_empty();
        if line.trim_matches([' ', '\t']).is_empty() {
            while lists
                .last()
                .is_some_and(|&(owner_depth, _, _, _, _, _)| owner_depth > quote_depth)
            {
                lists.pop();
            }
            depth = 0;
            paragraph = false;
        } else if let Some(item) = marker.captures(visible).filter(|_| {
            !thematic(visible)
                && (!paragraph
                    || quote_depth < depth
                    || lists.last().is_some_and(
                        |&(owner_depth, column, _, _source_prefix, _, _)| {
                            owner_depth == quote_depth && indent <= column
                        },
                    ))
        }) {
            rejected_comment_open = opens_rejected_comment;
            while lists
                .last()
                .is_some_and(|&(owner_depth, column, _, _source_prefix, _, _)| {
                    owner_depth > quote_depth || owner_depth == quote_depth && column > indent
                })
            {
                lists.pop();
            }
            if quote_depth < depth {
                if paragraph
                    && !lists.last().is_some_and(
                        |&(owner_depth, column, _, _source_prefix, _, _)| {
                            owner_depth == quote_depth && indent <= column
                        },
                    )
                {
                    let end = boundary
                        .match_indices('>')
                        .nth(quote_depth)
                        .map_or(boundary.len(), |(at, _)| at + 1);
                    insertions.push((line_offset, line_offset, format!("{}\n", &boundary[..end])));
                }
                depth = quote_depth;
                boundary = prefix.trim_end().to_owned();
            }
            let width = compact_djot_list_prefix(item.get(1).unwrap().as_str()).len();
            let compact = compact_djot_list_prefix(text);
            if compact != text {
                item_edit = Some((
                    line_offset + prefix.len() + raw_indent,
                    line_offset + line.len(),
                    compact.into_owned(),
                ));
            }
            if !lists
                .last()
                .is_some_and(|&(owner_depth, column, _, _source_prefix, _, _)| {
                    owner_depth == quote_depth && column == indent
                })
            {
                let target = lists
                    .last()
                    .filter(|&&(owner_depth, column, _, _, _, _)| {
                        owner_depth == quote_depth && indent > column
                    })
                    .map_or(0, |&(_, _, width, _, target, _)| target + width);
                lists.push((
                    quote_depth,
                    indent,
                    width,
                    source_prefix,
                    target,
                    item.get(1).unwrap().as_str().len(),
                ));
            }
            item_target = lists.last().map(|&(_, _, _, _, target, _)| target);
            let body_start = item.get(3).unwrap().start();
            let mut quote_body = &text[body_start..];
            let mut nested_depth = 0;
            let mut nested_prefix = String::new();
            while let Some(nested) = embedded.find(quote_body) {
                nested_depth += nested.as_str().bytes().filter(|&b| b == b'>').count();
                nested_prefix.push_str(nested.as_str());
                quote_body = &quote_body[nested.end()..];
                let Some(child) = marker.captures(quote_body) else {
                    break;
                };
                let child_width = compact_djot_list_prefix(child.get(1).unwrap().as_str()).len();
                let column = nested
                    .as_str()
                    .bytes()
                    .rev()
                    .take_while(|b| matches!(b, b' ' | b'\t'))
                    .count()
                    .saturating_sub(1);
                lists.push((
                    quote_depth + nested_depth,
                    column,
                    child_width,
                    line.len() - quote_body.len() - column,
                    column,
                    child.get(1).unwrap().as_str().len(),
                ));
                let child_body = child.get(3).unwrap().start();
                if !embedded.is_match(&quote_body[child_body..]) {
                    break;
                }
                nested_prefix.push_str(&" ".repeat(child_width));
                quote_body = &quote_body[child_body..];
            }
            if nested_depth > 0 {
                depth = quote_depth
                    + if quote_body.is_empty() {
                        0
                    } else {
                        nested_depth
                    };
                boundary = format!(
                    "{prefix}{}{}",
                    " ".repeat(item_target.unwrap_or(0) + width),
                    nested_prefix.trim_end()
                );
            } else if quote_depth > 0 {
                depth = quote_depth;
                boundary = prefix.trim_end().to_owned();
            }
            paragraph = !(quote_body.is_empty()
                || reference.is_match(quote_body)
                || thematic(quote_body)
                || quote_body.starts_with(":::")
                || heading.is_match(quote_body)
                || read_djot_word_attributes(quote_body, 0)
                    .is_some_and(|(end, _)| end == quote_body.len()));
        } else if quote_depth > 0 && (!paragraph || depth > 0) {
            if quote_depth >= depth || text.is_empty() || !paragraph {
                depth = quote_depth;
                boundary = prefix.trim_end().to_owned();
            }
            paragraph = !(text.is_empty()
                || reference.is_match(text)
                || (!paragraph
                    && visible.starts_with('{')
                    && read_djot_word_attributes(visible, 0)
                        .is_some_and(|(end, _)| end == visible.len()))
                || thematic(visible)
                || visible.starts_with(":::")
                || heading.is_match(visible));
        } else if reference.is_match(text)
            || heading.is_match(visible)
            || (!paragraph
                && visible.starts_with('{')
                && read_djot_word_attributes(visible, 0)
                    .is_some_and(|(end, _)| end == visible.trim_end().len()))
            || visible.starts_with('|')
            || visible.starts_with(":::")
            || thematic(visible)
        {
            depth = quote_depth;
            paragraph = false;
        } else {
            if !paragraph {
                depth = quote_depth;
            }
            paragraph = true;
        }
        if quote_depth > 0 && !has_definition_syntax {
            let mut owned_columns = vec![None; quote_depth];
            for &(owner_depth, column, width, _source_prefix, target, _) in lists
                .iter()
                .take_while(|&&(owner_depth, _, _, _, _, _)| owner_depth < quote_depth)
            {
                if owner_depth < quote_depth && column_at(owner_depth, _source_prefix) > column {
                    owned_columns[owner_depth] = Some(target + width);
                }
            }
            let mut normalized_prefix = String::new();
            for (level, owned_column) in owned_columns.into_iter().enumerate() {
                let column = if comment_continuation {
                    owned_column.map_or(columns[level], |column| columns[level].min(column - 1))
                } else {
                    owned_column.unwrap_or(0)
                };
                normalized_prefix.push_str(&" ".repeat(column));
                normalized_prefix.push('>');
                if level + 1 < quote_depth || !prefix.ends_with('>') {
                    normalized_prefix.push(' ');
                }
            }
            if normalized_prefix != prefix {
                if boundary.trim_end() == prefix.trim_end() {
                    boundary = normalized_prefix.trim_end().to_owned();
                } else if let Some(suffix) = boundary.strip_prefix(prefix) {
                    boundary = normalized_prefix.clone() + suffix;
                }
                insertions.push((line_offset, line_offset + prefix.len(), normalized_prefix));
            }
        }
        let native_indent = item_target.unwrap_or_else(|| {
            lists
                .last()
                .filter(|&&(owner_depth, column, _, _, _, _)| {
                    owner_depth == quote_depth && indent > column
                })
                .map_or(indent, |&(_, column, width, _, target, source_width)| {
                    target + width + indent.saturating_sub(column + source_width)
                })
        });
        if quote_depth > 0 && raw_indent != native_indent && !rows[row_index] {
            insertions.push((
                line_offset + prefix.len(),
                line_offset + prefix.len() + raw_indent,
                " ".repeat(native_indent),
            ));
        }
        if rows[row_index]
            && quote_depth > 0
            && !has_definition_syntax
            && !lists
                .iter()
                .any(|&(owner_depth, _, _, _, _, _)| owner_depth >= quote_depth)
            && indent > 0
        {
            insertions.push((
                line_offset + prefix.len(),
                line_offset + prefix.len() + raw_indent,
                String::new(),
            ));
        }
        if let Some(edit) = item_edit {
            insertions.push(edit);
        }
    }
    if insertions.is_empty() {
        return std::borrow::Cow::Borrowed(source);
    }
    let mut result = String::with_capacity(source.len());
    let mut start = 0;
    for (at, end, text) in insertions {
        result.push_str(&source[start..at]);
        result.push_str(&text);
        start = end;
    }
    result.push_str(&source[start..]);
    std::borrow::Cow::Owned(result)
}

struct NoteBuffer {
    depth: usize,
    column: usize,
    label: String,
    body: Vec<String>,
    paragraph: bool,
    blank: bool,
    paragraph_depth: usize,
    item_column: Option<usize>,
    slot: Option<String>,
}

pub(super) fn note_blocks(
    source: &str,
    metadata: &DjotFootnoteAttributeStrip,
    empty_note: &str,
) -> String {
    if !source.contains("[^") {
        return blocks(source);
    }
    let quote = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*").unwrap();
    let header = cached_regex!(r"^(\[\^[^\]\n]+\]:[ \t]*)(.*)$").unwrap();
    let first_header = cached_regex!(r"^\[\^[^\]\n]+\]:[ \t]*").unwrap();
    let item = cached_regex!(r"^(?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+").unwrap();
    let inline_header = cached_regex!(r"^((?:[-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))[ \t]+)((?:>[ \t](?:[ \t]*>[ \t])*)?)[ \t]*(\[\^[^\]\n]+\]:[ \t]*)(.*)$").unwrap();
    let block = cached_regex!(r"^(?:#{1,6}(?:[ \t]|$)|:{3,}|[|]|[`~]{3,}|[-*+][ \t]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)][ \t]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\)[ \t]|\[[^\]^\n][^\]\n]*\]:)").unwrap();
    let code = cached_regex!(r"^(`{3,}|~{3,})[ \t]*[A-Za-z0-9_+#.-]*$").unwrap();
    let mask = mask_code_and_destinations(source);
    let rows = djot_table_rows(source, &mask);
    let fence_mask = mask_djot_fences(source, None, &rows, true);
    let slot_prefix = djot_placeholder_prefix(source, "\0DJOTNOTESLOT\0");
    let mut main = Vec::<String>::new();
    let mut notes = Vec::<NoteBuffer>::new();
    let mut completed = Vec::<NoteBuffer>::new();
    let mut paragraph = false;
    let mut blank = true;
    let mut previous_depth = 0;
    let mut last_boundary = false;
    let mut main_item: Option<(usize, usize)> = None;
    let mut held_code: Option<(u8, usize, usize, Option<usize>)> = None;
    for (note_line_index, ((line, masked), fenced)) in source
        .split('\n')
        .zip(mask.split('\n'))
        .zip(fence_mask.split('\n'))
        .enumerate()
    {
        let physical_prefix = quote.find(line).unwrap().as_str();
        let physical_depth = physical_prefix.bytes().filter(|&b| b == b'>').count();
        let physical_rest = &line[physical_prefix.len()..];
        let physical_indent = physical_rest
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        let may_open_inline = blank
            || !paragraph
            || last_boundary
            || physical_depth < previous_depth
            || main_item.is_some_and(|(owner_depth, column)| {
                physical_depth <= owner_depth && physical_indent <= column
            })
            || notes.last().is_some_and(|note| {
                physical_depth <= note.depth
                    && (physical_indent <= note.column
                        || note
                            .item_column
                            .is_some_and(|column| physical_indent <= column))
            });
        let inline = may_open_inline
            .then(|| inline_header.captures(&physical_rest[physical_indent..]))
            .flatten();
        let virtual_prefix = inline
            .as_ref()
            .filter(|caps| !caps[2].is_empty())
            .map(|caps| {
                format!(
                    "{physical_prefix}{}{}{}",
                    " ".repeat(physical_indent),
                    &caps[1],
                    &caps[2]
                )
            });
        let prefix = virtual_prefix.as_deref().unwrap_or(physical_prefix);
        let depth = prefix.bytes().filter(|&b| b == b'>').count();
        let rest = &line[prefix.len()..];
        let indent = rest
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        let inline_width = inline
            .as_ref()
            .filter(|caps| caps[2].is_empty())
            .map_or(0, |caps| caps[1].len());
        let header_column = indent + inline_width;
        let text = &rest[indent + inline_width..];
        let visible = masked
            .get(prefix.len() + indent + inline_width..)
            .unwrap_or("");
        let quote_view = virtual_prefix.as_ref().map(|_| {
            let marker = &inline.as_ref().unwrap()[1];
            let start = physical_prefix.len() + physical_indent;
            format!(
                "{}{}{}",
                &line[..start],
                " ".repeat(marker.len()),
                &line[start + marker.len()..]
            )
        });
        let column_line = quote_view.as_deref().unwrap_or(line);
        let mut columns = Vec::with_capacity(depth + 1);
        let mut ends = Vec::with_capacity(depth);
        let mut at = 0;
        for level in 0..=depth {
            let start = at;
            while column_line
                .as_bytes()
                .get(at)
                .is_some_and(|b| matches!(b, b' ' | b'\t'))
            {
                at += 1;
            }
            columns.push(at - start);
            if level < depth {
                at += 1;
                if column_line
                    .as_bytes()
                    .get(at)
                    .is_some_and(|b| matches!(b, b' ' | b'\t'))
                {
                    at += 1;
                }
                ends.push(at);
            }
        }
        if held_code.is_some_and(|(_, _, host, floor)| {
            depth < host
                || floor.is_some_and(|floor| depth == host && indent < floor && !text.is_empty())
        }) {
            held_code = None;
        }
        let active = notes.last();
        let outside_main_item = active.is_none()
            && main_item.is_some_and(|(owner_depth, column)| {
                depth < owner_depth
                    || columns
                        .get(owner_depth)
                        .is_some_and(|current| *current <= column)
            });
        let can_define = inline.is_some()
            || blank
            || !paragraph
            || last_boundary
            || outside_main_item
            || depth < previous_depth
            || active.is_some_and(|n| depth <= n.depth && indent <= n.column);
        let head = can_define
            .then(|| header.captures(text))
            .flatten()
            .filter(|caps| {
                let colon = caps[1].trim_end_matches([' ', '\t']).len() - 1;
                masked.as_bytes().get(prefix.len() + header_column + colon) == Some(&b':')
                    || held_code.is_none()
                        && fenced != line
                        && code.is_match(&caps[2][djot_content_start(&caps[2])..])
            });
        while let Some(note) = notes.last() {
            if fenced != line && (held_code.is_some() || !code.is_match(text)) {
                break;
            }
            let column = if inline.is_some() && note.depth == physical_depth {
                physical_indent
            } else {
                columns.get(note.depth).copied().unwrap_or(indent)
            };
            let outside = depth < note.depth || column <= note.column;
            let starts_block = head.is_some()
                || code.is_match(text)
                || depth > note.depth
                || depth < note.depth && text.is_empty()
                || block.is_match(visible)
                || metadata.is_boundary(line);
            if outside
                && !(note.paragraph && !note.blank && !starts_block && depth <= note.depth)
                && (!text.is_empty() || depth != note.depth)
            {
                let closed = notes.pop().unwrap();
                if let Some(parent) = notes.last_mut() {
                    if (closed.blank || !closed.paragraph)
                        && parent.item_column.is_some_and(|column| indent <= column)
                        && !block.is_match(visible)
                        && !item.is_match(text)
                    {
                        parent.body.push(String::new());
                        parent.blank = true;
                    }
                    parent.paragraph = false;
                }
                completed.push(closed);
            } else {
                break;
            }
        }
        if let Some(caps) = head {
            if let Some(inline) = &inline {
                let dummy = format!(
                    "{physical_prefix}{}{}{}{empty_note}",
                    " ".repeat(physical_indent),
                    &inline[1],
                    &inline[2]
                );
                if let Some(parent) = notes.last_mut() {
                    let host = ends.get(parent.depth.wrapping_sub(1)).copied().unwrap_or(0);
                    let payload = &dummy[host..];
                    let padding = payload
                        .bytes()
                        .take_while(|b| matches!(b, b' ' | b'\t'))
                        .count();
                    parent
                        .body
                        .push(payload[padding.min(parent.column)..].to_owned());
                } else {
                    main.push(dummy);
                    main_item = Some((physical_depth, physical_indent));
                }
            }
            let parent = notes.last_mut();
            let slot = if parent.is_none() && depth == 0 && header_column == 0 {
                Some(format!("{slot_prefix}{}\0", completed.len()))
            } else {
                None
            };
            let separator = if let Some(parent) = parent {
                let kept = prefix
                    .get(ends.get(parent.depth.wrapping_sub(1)).copied().unwrap_or(0)..)
                    .unwrap_or("")
                    .trim_end()
                    .to_owned();
                if inline.is_none() {
                    parent.body.push(kept);
                }
                None
            } else {
                if depth > 0 && inline.is_none() {
                    main.push(prefix.trim_end().to_owned());
                }
                Some(())
            };
            if separator.is_some() {
                if let Some(slot) = &slot {
                    main.push(slot.clone());
                }
            }
            let body = &caps[2];
            if let Some(opener) = code.captures(&body[djot_content_start(body)..]) {
                held_code = Some((
                    opener[1].as_bytes()[0],
                    opener[1].len(),
                    depth,
                    Some(header_column + 1),
                ));
            }
            let quoted_item = quote
                .find(body)
                .is_some_and(|prefix| !prefix.is_empty() && item.is_match(&body[prefix.end()..]));
            let first_padding = if item.is_match(body) || quoted_item {
                caps[1].encode_utf16().count()
            } else if code.is_match(body) {
                2
            } else {
                0
            };
            let body_paragraph = !body.is_empty() && !block.is_match(body) && !code.is_match(body);
            notes.push(NoteBuffer {
                depth,
                column: header_column,
                label: caps[1].to_owned(),
                body: vec![format!("{}{body}", " ".repeat(first_padding))],
                item_column: item
                    .is_match(body)
                    .then_some(header_column + caps[1].encode_utf16().count()),
                paragraph_depth: depth
                    + body[..djot_content_start(body)]
                        .bytes()
                        .filter(|&b| b == b'>')
                        .count(),
                paragraph: body_paragraph || item.is_match(body) || starts_quote(body),
                blank: !body_paragraph && !item.is_match(body) && !starts_quote(body),
                slot,
            });
            let mut nested_body = body;
            while let Some(marker) = item.find(nested_body) {
                let after_marker = &nested_body[marker.end()..];
                let Some(nested) = first_header.find(after_marker) else {
                    break;
                };
                let parent = notes.last_mut().unwrap();
                let child_column = parent.column
                    + parent.label.encode_utf16().count()
                    + marker.as_str().encode_utf16().count();
                parent.body[0] = format!(
                    "{}{}{empty_note}",
                    " ".repeat(parent.label.encode_utf16().count()),
                    marker.as_str()
                );
                nested_body = &after_marker[nested.end()..];
                let child_item = item.is_match(nested_body);
                let child_paragraph = !nested_body.is_empty()
                    && !block.is_match(nested_body)
                    && !code.is_match(nested_body);
                let child_padding = if child_item {
                    nested.as_str().encode_utf16().count()
                } else if code.is_match(nested_body) {
                    2
                } else {
                    0
                };
                if let Some(opener) = code.captures(nested_body) {
                    held_code = Some((
                        opener[1].as_bytes()[0],
                        opener[1].len(),
                        depth,
                        Some(child_column + 1),
                    ));
                }
                notes.push(NoteBuffer {
                    depth,
                    column: child_column,
                    label: nested.as_str().to_owned(),
                    body: vec![if item
                        .find(nested_body)
                        .is_some_and(|marker| first_header.is_match(&nested_body[marker.end()..]))
                    {
                        String::new()
                    } else {
                        format!("{}{nested_body}", " ".repeat(child_padding))
                    }],
                    item_column: child_item
                        .then_some(child_column + nested.as_str().encode_utf16().count()),
                    paragraph_depth: depth,
                    paragraph: child_paragraph || child_item || starts_quote(nested_body),
                    blank: !child_paragraph && !child_item && !starts_quote(nested_body),
                    slot: None,
                });
            }
            paragraph = notes.last().unwrap().paragraph;
            blank = notes.last().unwrap().blank;
            previous_depth = depth;
            last_boundary = false;
            continue;
        }
        if fenced != line {
            if let Some((kind, width, host, _)) = held_code {
                let start = ends.get(host.wrapping_sub(1)).copied().unwrap_or(0);
                let content = line[start..].trim_matches([' ', '\t']);
                let run = content.bytes().take_while(|&b| b == kind).count();
                if run >= width && content[run..].trim_matches([' ', '\t']).is_empty() {
                    held_code = None;
                }
            } else if let Some(opener) = code.captures(&line[djot_content_start(line)..]) {
                held_code = Some((
                    opener[1].as_bytes()[0],
                    opener[1].len(),
                    depth,
                    notes.last().map(|note| note.column + 1),
                ));
            }
        }
        if let Some(note) = notes.last_mut() {
            let host = ends.get(note.depth.wrapping_sub(1)).copied().unwrap_or(0);
            let payload = &line[host..];
            let padding = payload
                .bytes()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            let removed = padding.min(note.column);
            let mut payload = payload[removed..].to_owned();
            let content_at = djot_content_start(line);
            if !rows[note_line_index]
                && line.as_bytes().get(content_at) == Some(&b'|')
                && masked.as_bytes().get(content_at) == Some(&b'|')
            {
                payload.insert(content_at - host - removed, '\\');
            }
            note.body.push(payload);
            let outside_item = note.item_column.is_some_and(|column| {
                columns.get(note.depth).copied().unwrap_or(indent) <= column
                    && (note.blank || depth > note.depth || block.is_match(visible))
            });
            if outside_item {
                note.item_column = None;
                note.paragraph = false;
                note.paragraph_depth = note.depth;
            }
            if item.is_match(text) && (!note.paragraph || outside_item) {
                note.item_column = Some(indent);
            }
            let literal_quote = depth > note.paragraph_depth && note.paragraph;
            note.blank = text.is_empty() && !literal_quote;
            if !note.paragraph && depth > note.depth {
                note.paragraph_depth = depth;
            }
            note.paragraph = !note.blank && fenced == line && !block.is_match(visible);
            paragraph = note.paragraph;
            blank = note.blank;
        } else {
            if outside_main_item
                && (blank || !paragraph || block.is_match(visible))
                && !item.is_match(text)
                && !text.is_empty()
            {
                main_item = None;
            }
            main.push(line.to_owned());
            blank = text.is_empty();
            if item.is_match(text) && (!paragraph || outside_main_item) {
                main_item = Some((depth, indent));
                let content = &text[djot_content_start(text)..];
                paragraph =
                    !content.is_empty() && !block.is_match(content) && !code.is_match(content);
            } else {
                paragraph = !blank && !block.is_match(visible);
            }
        }
        previous_depth = depth;
        last_boundary = metadata.is_boundary(line);
    }
    while let Some(note) = notes.pop() {
        completed.push(note);
    }
    if completed.is_empty() {
        return blocks(source);
    }
    let mut definitions = HashMap::<String, String>::new();
    let mut unplaced = Vec::new();
    for note in completed {
        let body = normalize_djot_fences(&note.body.join("\n"));
        let converted = blocks_with_slot(&body, None, Some(empty_note));
        let mut lines = converted.split('\n');
        let mut definition = format!(
            "{}{}",
            note.label,
            lines.next().unwrap_or("").trim_start_matches([' ', '\t'])
        );
        for line in lines {
            definition.push('\n');
            if !line.is_empty() {
                definition.push_str("  ");
                definition.push_str(line);
            }
        }
        if let Some(slot) = note.slot {
            definitions.insert(slot, definition);
        } else {
            unplaced.push(definition);
        }
    }
    let mut main_source = String::new();
    for (index, line) in main.iter().enumerate() {
        if index > 0 {
            main_source.push('\n');
        }
        main_source.push_str(line);
        if line.starts_with(&slot_prefix)
            && main
                .get(index + 1)
                .is_some_and(|next| code.is_match(next.trim_start_matches([' ', '\t'])))
        {
            main_source.push('\n');
        }
    }
    let main = normalize_djot_fences(&main_source);
    let normalized_main = blocks_with_slot(&main, Some(&slot_prefix), Some(empty_note));
    let mut result = unplaced.join("\n\n");
    if !result.is_empty() {
        result.push_str("\n\n");
    }
    for (n, line) in normalized_main.split('\n').enumerate() {
        if n > 0 {
            result.push('\n');
        }
        if let Some(definition) = definitions.remove(line) {
            result.push_str(&definition);
        } else {
            result.push_str(line);
        }
    }
    result
}

pub(super) fn blocks(source: &str) -> String {
    blocks_with_slot(source, None, None)
}

fn blocks_with_slot(source: &str, slot_prefix: Option<&str>, empty_note: Option<&str>) -> String {
    let initial_mask = mask_code_and_destinations(source);
    let row_source = slot_prefix.map(|prefix| {
        source
            .split('\n')
            .map(|line| {
                if line.starts_with(prefix) {
                    " ".repeat(line.len())
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    let initial_rows = djot_table_rows_with_empty_note(
        row_source.as_deref().unwrap_or(source),
        &initial_mask,
        empty_note,
    );
    let initial_fences = mask_djot_fences(source, None, &initial_rows, true);
    let has_definition_syntax = cached_regex!(r"(?:^|\n)(?:[ \t]*>(?:[ \t]|$))*[ \t]*:[ \t]")
        .unwrap()
        .is_match(&initial_mask);
    let normalized = quoted_list_boundaries(
        source,
        &initial_mask,
        &initial_fences,
        &initial_rows,
        slot_prefix,
        has_definition_syntax,
    );
    let changed = matches!(&normalized, std::borrow::Cow::Owned(_));
    let source = normalized.as_ref();
    let mask = if changed {
        mask_code_and_destinations(source)
    } else {
        initial_mask
    };
    let mut div_closers = HashMap::new();
    if source.contains(":::") {
        djot_inline_boundaries_with_div_closers(source, &mask, true, Some(&mut div_closers));
    }
    let mut line_offset = 0;
    let line_offsets: Vec<_> = source
        .split('\n')
        .map(|line| {
            let at = line_offset;
            line_offset += line.len() + 1;
            at
        })
        .collect();
    let masks: Vec<_> = mask.split('\n').collect();
    let lines: Vec<_> = source.split('\n').collect();
    let rows = if changed {
        let row_source = slot_prefix.map(|prefix| {
            source
                .split('\n')
                .map(|line| {
                    if line.starts_with(prefix) {
                        " ".repeat(line.len())
                    } else {
                        line.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        });
        djot_table_rows_with_empty_note(row_source.as_deref().unwrap_or(source), &mask, empty_note)
    } else {
        initial_rows
    };
    let fence_mask = if changed {
        mask_djot_fences(source, None, &rows, true)
    } else {
        initial_fences
    };
    let fence_masks: Vec<_> = fence_mask.split('\n').collect();
    let code_opener = cached_regex!(r"^(`{3,}|~{3,})[ \t]*=?[A-Za-z0-9_+#.-]*[ \t]*$").unwrap();
    let mut open_code: Option<(u8, usize, String)> = None;
    let pipe_pass = source.contains("\\|");
    let marker =
        cached_regex!(r"^([-*+]|(?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)[.)]|\((?:[0-9]+|[A-Za-z]|[ivxlcdm]+|[IVXLCDM]+)\))([ \t]+)(?:\[[ xX]\][ \t]+)?")
            .unwrap();
    let heading = cached_regex!(r"^#{1,6}(?:[ \t]|$)").unwrap();
    let reference = cached_regex!(r"^\[[^\]\n]+\]:").unwrap();
    let ordered_label = cached_regex!(r"[0-9A-Za-z]+").unwrap();
    let continuation_lines = table_continuation_lines(source);
    let mut result: Vec<String> = Vec::new();
    let mut lists: Vec<List> = Vec::new();
    let mut definition_columns = Vec::new();
    let mut definition_body = false;
    let mut divs: Vec<(usize, String, bool, Option<usize>, usize)> = Vec::new();
    let mut paragraph = false;
    let mut previous_blank = true;
    let mut in_heading = false;
    let mut in_quote = false;
    let mut quote_paragraph_depth = None;
    for (n, raw) in lines.iter().enumerate() {
        if slot_prefix.is_some_and(|prefix| raw.starts_with(prefix)) {
            paragraph = false;
            previous_blank = true;
            in_heading = false;
            result.push((*raw).to_owned());
            continue;
        }
        let (indent, indent_bytes) = leading_indent(raw);
        let text = &raw[indent_bytes..];
        let visible = masks[n].get(indent_bytes..).unwrap_or("");
        if rejected_comment_starts_block(visible) {
            let outside_list = lists.first().is_some_and(|list| indent <= list.source);
            let outside_quote = in_quote && !visible.starts_with('>');
            let outside_definition = definition_columns
                .last()
                .is_some_and(|column| indent < *column);
            if outside_definition {
                definition_columns.clear();
                definition_body = false;
                paragraph = false;
                previous_blank = true;
            }
            if in_heading || outside_list || outside_quote || outside_definition {
                result.push(raw[..indent_bytes].to_owned());
            }
            if outside_quote {
                in_quote = false;
                quote_paragraph_depth = None;
                paragraph = false;
                previous_blank = true;
            }
            if outside_list {
                lists.clear();
                paragraph = false;
                previous_blank = true;
            }
            in_heading = false;
        }
        let outer_source = lists.first().map(|list| list.source);
        let block = rows[n]
            || fence_masks[n] != *raw
            || heading.is_match(visible)
            || starts_quote(visible)
            || visible.starts_with(":::")
            || visible.starts_with('|')
            || thematic(visible);
        if !raw.trim_matches([' ', '\t']).is_empty() {
            while divs.last().is_some_and(|(_, _, _, owner, _)| {
                owner.is_some_and(|column| {
                    indent < column && (marker.is_match(visible) || block || previous_blank)
                })
            }) {
                let (width, prefix, _, _, _) = divs.pop().unwrap();
                let before = result
                    .iter()
                    .rposition(|line| !line.trim_matches([' ', '\t']).is_empty())
                    .map_or(0, |n| n + 1);
                result.insert(before, format!("{prefix}{}", ":".repeat(width)));
                paragraph = false;
                quote_paragraph_depth = None;
            }
        }
        if block {
            while lists.last().is_some_and(|list| indent <= list.source) {
                lists.pop();
                paragraph = false;
                in_heading = false;
                if let Some(parent) = lists.last_mut() {
                    parent.loose = true;
                }
            }
            if previous_blank {
                for list in &mut lists {
                    if indent > list.source {
                        list.loose = true;
                    }
                }
            }
        }
        if previous_blank
            && definition_columns
                .last()
                .is_some_and(|column| indent >= *column)
        {
            definition_body = true;
        }
        if (block || definition_body && marker.is_match(visible))
            && definition_columns
                .last()
                .is_some_and(|column| indent < *column)
        {
            definition_columns.clear();
            definition_body = false;
            paragraph = false;
            in_heading = false;
        }
        let target_indent = if !has_definition_syntax && quoted(raw).0 > 0 && fence_masks[n] == *raw
        {
            indent
        } else {
            lists
                .last()
                .filter(|list| indent > list.source)
                .map_or(indent, |list| {
                    list.target_column + indent.saturating_sub(list.column)
                })
        };
        let display = if target_indent == indent {
            (*raw).to_owned()
        } else {
            format!("{}{text}", " ".repeat(target_indent))
        };
        if raw.trim_matches([' ', '\t']).is_empty() {
            quote_paragraph_depth = None;
            paragraph = false;
            in_heading = false;
            in_quote = false;
            previous_blank = true;
            result.push(display.clone());
            continue;
        }
        if visible.trim().is_empty() {
            if fence_masks[n].trim().is_empty() {
                if let Some((kind, width, _)) = &open_code {
                    let length = text.bytes().take_while(|byte| byte == kind).count();
                    if length >= *width && text[length..].trim().is_empty() {
                        open_code = None;
                    }
                } else if let Some(caps) = code_opener.captures(text) {
                    open_code = Some((
                        caps[1].as_bytes()[0],
                        caps[1].len(),
                        " ".repeat(target_indent),
                    ));
                }
                paragraph = false;
                in_heading = false;
            } else {
                paragraph = !in_heading;
            }
            previous_blank = false;
            result.push(display.clone());
            continue;
        }
        if continuation_lines.contains(&(n + 1)) {
            result.push(display.clone());
            paragraph = false;
            previous_blank = false;
            continue;
        }
        if quoted(raw).0 > 0
            && (in_quote || !paragraph || n > 0 && masks[n - 1].trim_start().starts_with('>'))
        {
            let (depth, body) = quoted(raw);
            let block_body = body.trim_start();
            if rejected_comment_starts_block(block_body) {
                let owned = lines[..n]
                    .iter()
                    .rev()
                    .take_while(|line| !quoted(line).1.trim().is_empty())
                    .any(|line| {
                        let (parent_depth, parent_body) = quoted(line);
                        parent_depth == depth
                            && (marker.is_match(parent_body.trim_start())
                                || heading.is_match(parent_body.trim_start()))
                    });
                if owned || depth < quote_paragraph_depth.unwrap_or(depth) {
                    result.push(raw[..raw.len() - body.len()].trim_end().to_owned());
                    quote_paragraph_depth = None;
                }
            }
            if let Some(held) =
                quote_paragraph_depth.filter(|&held| depth > held && !body.trim().is_empty())
            {
                let mut at = 0;
                for _ in 0..held {
                    let rest = &raw[at..];
                    at += rest.len() - rest.trim_start_matches([' ', '\t']).len() + 1;
                    if raw.as_bytes().get(at) == Some(&b' ') {
                        at += 1;
                    }
                }
                let mut literal = display.clone();
                let at = if target_indent == indent {
                    at
                } else {
                    at - indent_bytes + target_indent
                };
                literal.insert(at, '\\');
                result.push(literal);
                previous_blank = false;
                continue;
            }
            let metadata_body = quote_paragraph_depth.is_none()
                && (reference.is_match(block_body)
                    || block_body.starts_with('{')
                        && read_djot_word_attributes(block_body, 0)
                            .is_some_and(|(end, _)| end == block_body.len()));
            quote_paragraph_depth = (!body.trim().is_empty()
                && !metadata_body
                && !heading.is_match(block_body)
                && !marker.is_match(block_body)
                && !block_body.starts_with(":::"))
            .then_some(depth.max(quote_paragraph_depth.unwrap_or(depth)));
            result.push(display.clone());
            in_quote = true;
            paragraph = false;
            previous_blank = body.trim().is_empty();
            continue;
        }
        if !paragraph && definition_term(raw).is_some() {
            while definition_columns
                .last()
                .is_some_and(|column| *column >= indent + 2)
            {
                definition_columns.pop();
            }
            definition_columns.push(indent + 2);
            definition_body = false;
        } else if previous_blank {
            while definition_columns
                .last()
                .is_some_and(|column| indent < *column)
            {
                definition_columns.pop();
            }
        }
        if let Some(item) = marker
            .captures(visible)
            .and_then(|_| marker.captures(text))
            .filter(|_| !thematic(text))
        {
            if in_quote && !previous_blank {
                if !paragraph && !lists.last().is_some_and(|list| indent <= list.source) {
                    result.push(String::new());
                }
                in_quote = false;
                quote_paragraph_depth = None;
            }
            let previous_list = lists.last().cloned();
            let ends_nested = previous_list
                .as_ref()
                .is_some_and(|list| indent < list.source);
            while lists.last().is_some_and(|list| indent < list.source) {
                lists.pop();
                paragraph = false;
                in_heading = false;
            }
            if let Some(parent) = lists
                .last_mut()
                .filter(|list| previous_blank && indent == list.source)
            {
                if ends_nested && !parent.loose {
                    while result
                        .last()
                        .is_some_and(|line| line.trim_matches([' ', '\t']).is_empty())
                    {
                        result.pop();
                    }
                    let start = previous_list.as_ref().unwrap().start;
                    if !parent.lazy
                        && start > 0
                        && result
                            .get(start - 1)
                            .is_some_and(|line| line.trim_matches([' ', '\t']).is_empty())
                    {
                        result.remove(start - 1);
                    }
                } else {
                    parent.loose = true;
                }
            }
            let top = lists.last().cloned();
            if let Some(parent) = top.as_ref().filter(|list| indent > list.source) {
                if !previous_blank && paragraph {
                    let mut literal = text.to_owned();
                    if matches!(item[1].as_bytes()[0], b'-' | b'*' | b'+') {
                        literal.insert(0, '\\');
                    } else if let Some(at) = literal.find(['.', ')']) {
                        literal.insert(at, '\\');
                    }
                    result.push(format!("{}{literal}", " ".repeat(parent.target_column)));
                    lists.last_mut().unwrap().lazy = true;
                    continue;
                }
            }
            let nested = top.as_ref().is_some_and(|list| indent > list.source);
            if top.is_none() && paragraph && !previous_blank {
                result.push(if matches!(item[1].as_bytes()[0], b'-' | b'*' | b'+') {
                    format!("{}\\{text}", &raw[..indent_bytes])
                } else {
                    display.clone()
                });
                continue;
            }
            let bullet = item[1].as_bytes()[0];
            let is_bullet = matches!(bullet, b'-' | b'*' | b'+');
            let kind = if is_bullet {
                item[1].to_owned()
            } else {
                ordered_label.replace(&item[1], "1").into_owned()
            };
            let mut target = if definition_columns
                .last()
                .is_some_and(|column| indent >= *column)
            {
                indent
            } else {
                0
            };
            let mut written = if is_bullet {
                if bullet == b'+' {
                    b'-'
                } else {
                    bullet
                }
            } else {
                b'-'
            };
            let mut separate = false;
            if let Some(parent) = &top {
                if nested {
                    target = parent.target_column;
                } else {
                    target = parent.target;
                    written = parent.written;
                    if kind != parent.kind {
                        written = if parent.written == b'-' { b'*' } else { b'-' };
                        separate = true;
                    }
                    lists.pop();
                }
            }
            if separate && result.last().is_some_and(|line| !line.is_empty()) {
                result.push(String::new());
            }
            let list_start = top
                .as_ref()
                .filter(|_| !nested && !separate)
                .map_or(result.len(), |list| list.start);
            let marker_end = item.get(2).unwrap().end();
            let payload_start = item.get(0).unwrap().end();
            let native_marker_end = compact_djot_list_prefix(&text[..marker_end]).len();
            let mut payload = compact_djot_list_prefix(text).into_owned();
            if is_bullet {
                payload.replace_range(..1, &(written as char).to_string());
            }
            if top
                .as_ref()
                .is_some_and(|list| !nested && list.loose && list.lazy)
                && result
                    .last()
                    .is_some_and(|line| !line.trim_matches([' ', '\t']).is_empty())
            {
                result.push(String::new());
            }
            result.push(format!("{}{payload}", " ".repeat(target)));
            lists.push(List {
                source: indent,
                column: indent + marker_end,
                target,
                target_column: target + native_marker_end,
                written,
                kind,
                start: list_start,
                lazy: false,
                loose: !nested && top.as_ref().is_some_and(|list| list.loose),
            });
            in_quote = quoted(&text[payload_start..]).0 > 0;
            let item_body = &text[payload_start..];
            let div_width = item_body.bytes().take_while(|b| *b == b':').count();
            let item_div = div_width >= 3;
            if item_div {
                divs.push((
                    div_width,
                    " ".repeat(target + native_marker_end),
                    crate::parse::lint_invalid_container_metadata(item_body),
                    Some(indent + marker_end),
                    line_offsets[n],
                ));
            }
            in_heading = heading.is_match(item_body);
            paragraph = !in_quote
                && !item_div
                && !in_heading
                && !reference.is_match(item_body)
                && !read_djot_word_attributes(item_body, 0)
                    .is_some_and(|(end, _)| end == item_body.len())
                && !empty_note.is_some_and(|marker| item_body.trim() == marker);
            previous_blank = item_div;
            continue;
        }
        if previous_blank {
            while lists.last().is_some_and(|list| indent <= list.source) {
                lists.pop();
            }
        }
        let width = text.bytes().take_while(|b| *b == b':').count();
        if width >= 3 && visible.starts_with(':') {
            let bare = text[width..].trim().is_empty();
            if let Some(starts) = div_closers.get(&line_offsets[n]) {
                drop_orphan_attribute_line(&mut result);
                let outer_start = *starts.last().unwrap();
                let mut matched = 0;
                while divs.last().is_some_and(|owner| owner.4 >= outer_start) {
                    let (open, prefix, _, _, start) = divs.pop().unwrap();
                    while starts.get(matched).is_some_and(|&open| open > start) {
                        matched += 1;
                    }
                    if starts.get(matched) == Some(&start) {
                        result.push(format!("{prefix}{}", ":".repeat(open)));
                        matched += 1;
                    }
                }
                paragraph = false;
            } else if bare && divs.iter().any(|(open, _, _, _, _)| width >= *open) {
                let boundary = if divs.iter().any(|(_, _, invalid, _, _)| *invalid) {
                    divs.iter()
                        .rposition(|(open, _, _, _, _)| width >= *open)
                        .unwrap()
                } else {
                    divs.iter()
                        .position(|(open, _, _, _, _)| width >= *open)
                        .unwrap()
                };
                drop_orphan_attribute_line(&mut result);
                while divs.len() > boundary {
                    let (open, prefix, _, _, _) = divs.pop().unwrap();
                    result.push(format!("{prefix}{}", ":".repeat(open)));
                }
                paragraph = false;
            } else if paragraph || bare && !divs.is_empty() {
                result.push(format!("{}\\{text}", &raw[..indent_bytes]));
            } else {
                divs.push((
                    width,
                    raw[..indent_bytes].to_owned(),
                    crate::parse::lint_invalid_container_metadata(text),
                    lists
                        .last()
                        .filter(|list| indent > list.source)
                        .map(|list| list.column),
                    line_offsets[n],
                ));
                result.push(display.clone());
                paragraph = false;
            }
        } else if paragraph
            && (heading.is_match(visible)
                || starts_quote(visible)
                || visible.starts_with('|') && !pipe_pass
                || thematic(visible) && !(visible.contains('*') && visible.contains('-')))
        {
            result.push(format!("{}\\{text}", &raw[..indent_bytes]));
        } else if heading.is_match(visible) {
            in_heading = true;
            let level = text.bytes().take_while(|b| *b == b'#').count();
            result.push(format!(
                "{}{}{}",
                if lists.is_empty() && definition_columns.is_empty() {
                    String::new()
                } else {
                    " ".repeat(target_indent)
                },
                "#".repeat(level),
                if text[level..].trim().is_empty() {
                    text[level..].to_owned()
                } else {
                    format!(" {}", text[level..].trim_start())
                }
            ));
            paragraph = false;
        } else if thematic(visible) {
            result.push(
                if !paragraph && lists.is_empty() && definition_columns.is_empty() {
                    "***".to_owned()
                } else {
                    display.clone()
                },
            );
            paragraph = false;
        } else if rows[n] {
            let leading = n == 0 || !rows[n - 1];
            if !(leading
                && separator(text)
                && !text.contains(':')
                && rows.get(n + 1) == Some(&true))
            {
                result.push(if lists.is_empty() && definition_columns.is_empty() {
                    text.to_owned()
                } else {
                    display.clone()
                });
            }
            paragraph = false;
        } else {
            let attribute = text.starts_with('{')
                && read_djot_word_attributes(text, 0).is_some_and(|(end, _)| end == text.len());
            result.push(
                if previous_blank
                    && lists.is_empty()
                    && outer_source.is_some_and(|source| indent <= source)
                {
                    text.to_owned()
                } else {
                    display.clone()
                },
            );
            if previous_blank && !attribute && !reference.is_match(text) {
                for list in &mut lists {
                    if indent > list.source {
                        list.loose = true;
                    }
                }
            }
            paragraph = !in_heading && (paragraph || !(attribute || reference.is_match(text)));
        }
        previous_blank = false;
    }
    if !divs.is_empty() {
        let trailing = result.last().is_some_and(|line| line.is_empty());
        if trailing {
            result.pop();
        }
        drop_orphan_attribute_line(&mut result);
        if trailing && !result.last().is_some_and(|line| line.is_empty()) {
            result.push(String::new());
        }
    }
    let mut output = result.join("\n");
    if !divs.is_empty() {
        if let Some((kind, width, prefix)) = open_code {
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(&format!(
                "{prefix}{}\n",
                (kind as char).to_string().repeat(width)
            ));
        }
    }
    while let Some((width, prefix, _, _, _)) = divs.pop() {
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&format!("{prefix}{}\n", ":".repeat(width)));
    }
    output
}

pub(super) fn table_edges(source: &str) -> String {
    let mask = mask_code_and_destinations(source);
    let rows = djot_table_rows(source, &mask);
    let mut escaped = false;
    let mut offset = 0;
    let masked: Vec<_> = source
        .split('\n')
        .map(|line| {
            // A line the mask hides is code, so its pipes are payload, not table edges.
            let hidden = mask.as_bytes().get(offset) != Some(&b'|');
            offset += line.len() + 1;
            hidden
        })
        .collect();
    let mut lines: Vec<_> = source
        .split('\n')
        .enumerate()
        .map(|(n, line)| {
            if !rows[n] && !masked[n] && line.starts_with("|`") && line.trim_end().ends_with('|') {
                escaped = true;
                format!("\\{line}")
            } else {
                line.to_owned()
            }
        })
        .collect();
    if escaped && source.trim_start_matches('\n').starts_with("|`") {
        while lines.first().is_some_and(|line| line.is_empty()) {
            lines.remove(0);
        }
    }
    lines.join("\n")
}

fn protected_mask(source: &str) -> Vec<u8> {
    mask_djot_opaque(
        &mask_djot_fences(source, None, &[], false),
        OpaqueOptions {
            code: false,
            unclosed_code: false,
            ..OpaqueOptions::default()
        },
    )
}

pub(super) fn inline(source: &str) -> String {
    let mask = mask_code_and_destinations(source);
    let protected = protected_mask(source);
    let comments: HashMap<_, _> = mask_djot_opaque_with_comments(
        &mask_djot_fences(source, None, &[], false),
        OpaqueOptions::default(),
    )
    .1
    .into_iter()
    .collect();
    let boundary = cached_regex!(r"\n[ \t]*(?:>[ \t]*)*\n").unwrap();
    let bytes = source.as_bytes();
    let mut breaks: Vec<_> = if bytes.contains(&b'`') {
        boundary.find_iter(source).map(|m| m.start()).collect()
    } else {
        Vec::new()
    };
    if bytes.contains(&b'`') && bytes.contains(&b'\n') {
        breaks.extend(
            djot_inline_boundaries_with_code_scopes(
                source,
                &String::from_utf8_lossy(&protected),
                true,
            )
            .into_iter()
            .filter(|&at| at > 0 && at < bytes.len() && bytes[at - 1] == b'\n' && bytes[at] != b'|')
            .map(|at| at - 1),
        );
        breaks.sort_unstable();
    }
    let mut paragraph = 0;
    let mut output = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = comments.get(&i).copied() {
            if bytes[end - 2] != b'%' && source[i + 2..end].contains('{') {
                output.push_str("{%%}");
                i = end;
                continue;
            }
        }
        if bytes[i] == b'`' && protected[i] == b'`' && !is_escaped(bytes, i) {
            let run = bytes[i..].iter().take_while(|b| **b == b'`').count();
            while breaks.get(paragraph).is_some_and(|end| *end <= i) {
                paragraph += 1;
            }
            let paragraph_end = breaks.get(paragraph).copied().unwrap_or(bytes.len());
            if let Some(end) = find_backtick_close(&bytes[..paragraph_end], i + run, run) {
                let payload = &source[i + run..end];
                let fenced = run >= 3
                    && payload.starts_with('\n')
                    && source[source[..i].rfind('\n').map_or(0, |at| at + 1)..i]
                        .trim()
                        .is_empty();
                output.push_str(&source[i..i + run]);
                let pad = !fenced
                    && payload.starts_with(' ')
                    && payload.ends_with(' ')
                    && !payload.trim().is_empty()
                    && !(payload.trim().starts_with('`') && payload.trim().ends_with('`'));
                if pad {
                    output.push(' ');
                }
                output.push_str(payload);
                if pad {
                    output.push(' ');
                }
                output.push_str(&source[end..end + run]);
                i = end + run;
                continue;
            }
            output.push_str(&source[i..paragraph_end]);
            i = paragraph_end;
            continue;
        }
        if mask.as_bytes()[i] == bytes[i] && protected[i] == bytes[i] && !is_escaped(bytes, i) {
            if bytes[i] == b'{' {
                if let Some((end, _)) = read_djot_word_attributes(source, i) {
                    output.push_str(&source[i..end]);
                    i = end;
                    continue;
                }
            }
            if source[i..].starts_with("{}") {
                if i > 0 && mask.as_bytes()[i - 1] == b']' && !is_escaped(bytes, i - 1) {
                    output.push_str("{}");
                    i += 2;
                    continue;
                }
                let mut end = i + 2;
                while source[end..].starts_with("{}") {
                    end += 2;
                }
                if i > 0
                    && matches!(bytes[i - 1], b'_' | b'*')
                    && bytes.get(end) == Some(&bytes[i - 1])
                {
                    output.push_str(&source[i..end]);
                    i = end;
                    continue;
                }
                let empty_line =
                    (i == 0 || bytes[i - 1] == b'\n') && bytes.get(i + 2) == Some(&b'\n');
                i += if empty_line { 3 } else { 2 };
                continue;
            }
            let quote = [("{'", "‘"), ("'}", "’"), ("{\"", "“"), ("\"}", "”")]
                .into_iter()
                .find(|(token, _)| source[i..].starts_with(token));
            if let Some((token, glyph)) = quote {
                output.push_str(glyph);
                i += token.len();
                continue;
            }
            if bytes[i] == b'\\' {
                let mut end = i + 1;
                while matches!(bytes.get(end), Some(b' ' | b'\t')) {
                    end += 1;
                }
                let line_start = source[..i].rfind('\n').map_or(0, |at| at + 1);
                let prefix = source[line_start..i].trim();
                let fence = prefix.len() >= 3 && prefix.bytes().all(|byte| byte == b':');
                if bytes.get(end) == Some(&b'\n') && output.ends_with([' ', '\t']) && !fence {
                    let mut trim = output.len();
                    while trim > 0
                        && matches!(output.as_bytes()[trim - 1], b' ' | b'\t')
                        && !is_escaped(output.as_bytes(), trim - 1)
                    {
                        trim -= 1;
                    }
                    output.truncate(trim);
                    output.push_str("\\\n");
                    i = end + 1;
                    continue;
                }
            }
        }
        let ch = source[i..].chars().next().unwrap();
        output.push(ch);
        i += ch.len_utf8();
    }
    output
}

fn definition_label(text: &str) -> String {
    text.trim()
        .split([' ', '\t', '\r', '\n'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn label(text: &str) -> String {
    let decoded = cached_regex!(r"\\([ \t!-/:-@\[-`{-~])")
        .unwrap()
        .replace_all(text, "$1");
    definition_label(&decoded)
}

fn reference_label(text: &str, explicit: &str) -> String {
    if !explicit.is_empty() {
        return label(explicit);
    }
    if !text.bytes().any(|b| b"_*`~^\\".contains(&b)) {
        return label(text);
    }
    let converted = emphasis::convert(text, &emphasis_mask(text), str::to_owned);
    let options = crate::Options {
        smart_typography: crate::SmartTypographyMode::Source,
        ..Default::default()
    };
    let nodes = crate::parse::parse_inline_with_options(&converted, &options);
    let mut pending: Vec<_> = nodes.iter().rev().collect();
    let mut text = String::new();
    while let Some(node) = pending.pop() {
        use crate::ast::InlineNode;
        let children: Option<&[InlineNode]> = match node {
            InlineNode::NonBreakingSpace(_) => continue,
            InlineNode::Emphasis(node) => Some(&node.children),
            InlineNode::Span(node) => Some(&node.children),
            InlineNode::Link(node) => Some(&node.children),
            InlineNode::Extension(node) => Some(&node.children),
            _ => None,
        };
        if let Some(children) = children {
            pending.extend(children.iter().rev());
        } else {
            text.push_str(&crate::render::plain_inlines_typography(
                std::slice::from_ref(node),
                crate::SmartTypographyMode::Source,
            ));
        }
    }
    definition_label(&text)
}

fn cached_reference_label<'a>(
    text: &'a str,
    explicit: &str,
    cache: &mut HashMap<&'a str, String>,
) -> String {
    const MAX_CACHED_REFERENCE_LABEL_BYTES: usize = 256;
    if !explicit.is_empty() {
        return label(explicit);
    }
    if !text.bytes().any(|byte| b"_*`~^\\".contains(&byte)) {
        return label(text);
    }
    // Bound retained values when nested labels overlap in the source.
    if text.len() > MAX_CACHED_REFERENCE_LABEL_BYTES {
        return reference_label(text, explicit);
    }
    cache
        .entry(text)
        .or_insert_with(|| reference_label(text, explicit))
        .clone()
}

fn attributes(text: &str) -> Vec<(String, Vec<String>)> {
    let mut result: Vec<(String, Vec<String>)> = Vec::new();
    let mut slots: HashMap<String, usize> = HashMap::new();
    let mut at = 0;
    while let Some((end, _, tokens)) = read_djot_attribute_tokens(text, at) {
        for token in tokens {
            if let Some(&slot) = slots.get(&token.key) {
                let values = &mut result[slot].1;
                if !token.append {
                    values.clear();
                }
                values.push(token.source);
            } else {
                slots.insert(token.key.clone(), result.len());
                result.push((token.key, vec![token.source]));
            }
        }
        at = end;
        if at == text.len() {
            break;
        }
    }
    result
}

struct ReferenceUse<'a> {
    start: usize,
    end: usize,
    text: &'a str,
    label: &'a str,
}

fn reference_mask(source: &str) -> String {
    let mut mask =
        mask_djot_forms_with_options(source, false, true, None, &[], OpaqueOptions::default())
            .into_bytes();
    let mut at = 0;
    while at < source.len() {
        if mask[at] == b'{' && !is_escaped(source.as_bytes(), at) {
            if let Some((end, _)) = read_djot_word_attributes(source, at) {
                blank_out(&mut mask, at, end);
                at = end;
                continue;
            }
        }
        at += 1;
    }
    String::from_utf8(mask).unwrap()
}

fn reference_uses<'a>(source: &'a str, mask: &str) -> Vec<ReferenceUse<'a>> {
    let bytes = source.as_bytes();
    let mut table_pipes = HashSet::new();
    if source.contains('|') {
        let pipe_mask = mask_djot_forms_with_options(
            source,
            false,
            true,
            None,
            &[],
            OpaqueOptions {
                destinations: false,
                autolinks: false,
                attribute_values: false,
                ..Default::default()
            },
        );
        let rows = djot_table_rows(source, &pipe_mask);
        let mut offset = 0;
        for (n, line) in source.split('\n').enumerate() {
            if rows[n] {
                let mut escaped = false;
                for (at, byte) in line.bytes().enumerate() {
                    if byte == b'|' && pipe_mask.as_bytes()[offset + at] == b'|' && !escaped {
                        table_pipes.insert(offset + at);
                    }
                    escaped = byte == b'\\' && !escaped;
                }
            }
            offset += line.len() + 1;
        }
    }
    let mut stack = Vec::new();
    let mut uses = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if table_pipes.contains(&i) {
            stack.clear();
        }
        if mask.as_bytes()[i] != bytes[i] {
            i += 1;
            continue;
        }
        if bytes[i] == b'\n' && blank_line_follows(bytes, i) {
            stack.clear();
        }
        if bytes[i] == b'[' {
            stack.push(i);
        }
        if bytes[i] == b']' {
            if let Some(start) = stack.pop() {
                if bytes.get(start + 1) != Some(&b'^') && bytes.get(i + 1) == Some(&b'[') {
                    let mut end = i + 2;
                    while end < bytes.len()
                        && mask.as_bytes()[end] != b']'
                        && mask.as_bytes()[end] != b'['
                    {
                        if table_pipes.contains(&end) {
                            break;
                        }
                        if bytes[end] == b'\n' && blank_line_follows(bytes, end) {
                            break;
                        }
                        if bytes[end] == b'\\' {
                            end += 1;
                        }
                        end += 1;
                    }
                    if mask.as_bytes().get(end) == Some(&b']') {
                        uses.push(ReferenceUse {
                            start,
                            end: end + 1,
                            text: &source[start + 1..i],
                            label: &source[i + 2..end],
                        });
                        i = end;
                    }
                }
            }
        }
        i += 1;
    }
    uses
}

pub(super) fn references(source: &str) -> String {
    let definition = cached_regex!(r"(?m)^\[([^\[\]\n^][^\[\]\n]*)\]:[ \t]*(\S*)[ \t]*$").unwrap();
    let mask = reference_mask(source);
    let mut definitions = HashMap::new();
    let mut removed = Vec::new();
    let mut definition_ranges: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    let mut definition_keys = Vec::new();
    let mut indented_definitions = Vec::new();
    let mut line_offset = 0;
    let mut valid_definition_lines = None;
    for (line_number, line) in source.split_inclusive('\n').enumerate() {
        let line_start = line_offset;
        line_offset += line.len();
        let content_start = djot_content_start(line);
        let Some(c) = definition.captures(line[content_start..].trim_end_matches('\n')) else {
            continue;
        };
        if content_start > 0
            && !valid_definition_lines
                .get_or_insert_with(|| reference_definitions(source, &mask).1)
                .contains(&line_number)
        {
            continue;
        }
        let start = line_start + content_start;
        let definition_end = start + c.get(0).unwrap().end();
        if mask.as_bytes()[start] != b'[' {
            continue;
        }
        if content_start > 0 && line[..content_start].trim().is_empty() {
            indented_definitions.push((line_start, start, definition_end, definition_label(&c[1])));
        }
        let mut previous_end = line_start;
        let mut attr_parts = Vec::new();
        while let Some(previous) = source[..previous_end].strip_suffix('\n') {
            let previous_start = previous.rfind('\n').map_or(0, |at| at + 1);
            let previous_line = &previous[previous_start..];
            let attribute_start = djot_content_start(previous_line);
            let attribute_content = &previous_line[attribute_start..];
            let attribute_prefix = &previous_line[..attribute_start];
            let prefix = &line[..content_start];
            let opens_container = prefix.bytes().any(|byte| b"-*+.):".contains(&byte))
                || prefix.bytes().filter(|&byte| byte == b'>').count()
                    > attribute_prefix
                        .bytes()
                        .filter(|&byte| byte == b'>')
                        .count();
            if opens_container
                || !attribute_content.starts_with('{')
                || read_djot_word_attributes(attribute_content, 0)
                    .map_or(true, |(end, _)| end != attribute_content.len())
            {
                break;
            }
            attr_parts.push(attribute_content);
            removed.push((
                previous_start + attribute_start,
                previous_start + previous_line.len() + usize::from(attribute_start == 0),
                if attribute_start > 0 {
                    "{%%}".to_owned()
                } else {
                    String::new()
                },
            ));
            previous_end = previous_start;
        }
        attr_parts.reverse();
        let attr_text = attr_parts.join("");
        let key = definition_label(&c[1]);
        definition_keys.push(key.clone());
        definition_ranges
            .entry(key.clone())
            .or_default()
            .push((start, definition_end));
        definitions.insert(
            key,
            (
                c[2].to_owned(),
                attributes(&attr_text),
                !attr_parts.is_empty(),
            ),
        );
    }
    if definitions.is_empty() {
        return source.to_owned();
    }
    let uses = reference_uses(source, &mask);
    let mut label_cache = HashMap::new();
    let mut aliases = HashMap::new();
    if definitions
        .keys()
        .any(|key| key.contains('\\') || key.contains('|'))
    {
        let mut occupied: HashSet<String> = definitions.keys().cloned().collect();
        for reference in &uses {
            occupied.insert(cached_reference_label(
                reference.text,
                reference.label,
                &mut label_cache,
            ));
        }
        let alias_keys: Vec<_> = definition_keys
            .into_iter()
            .filter(|key| {
                definitions.contains_key(key) && (key.contains('\\') || key.contains('|'))
            })
            .collect();
        let mut serial = 0;
        for key in alias_keys {
            if aliases.contains_key(&key) {
                continue;
            }
            let alias = loop {
                serial += 1;
                let candidate = format!("carve-djot-reference-{serial}");
                if occupied.insert(candidate.clone()) {
                    break candidate;
                }
            };
            aliases.insert(key, alias);
        }
    }
    let trim_definition_space = !removed.is_empty();
    let mut edits = removed;
    let mut moved_definitions = Vec::new();
    for (line_start, start, end, key) in indented_definitions {
        if definition_ranges
            .get(&key)
            .and_then(|ranges| ranges.last())
            .copied()
            != Some((start, end))
        {
            continue;
        }
        let close = source[start..end].find("]:").unwrap();
        let name = aliases
            .get(&key)
            .map(String::as_str)
            .unwrap_or(&source[start + 1..start + close]);
        moved_definitions.push(format!("[{name}{}", &source[start + close..end]));
        edits.push((
            line_start,
            end,
            format!("{}{{%%}}", &source[line_start..start]),
        ));
    }
    for ranges in definition_ranges.values() {
        for &(start, end) in ranges.iter().take(ranges.len().saturating_sub(1)) {
            edits.push((start, end, "{%%}".to_owned()));
        }
    }
    for (key, alias) in &aliases {
        for &(start, end) in definition_ranges.get(key).into_iter().flatten() {
            if let Some(close) = source[start..end].find("]:") {
                edits.push((start + 1, start + close, alias.clone()));
            }
        }
    }
    for reference in uses {
        let start = reference.start;
        let mut end = reference.end;
        let key = cached_reference_label(reference.text, reference.label, &mut label_cache);
        let Some((target, inherited, has_attribute_lines)) = definitions.get(&key) else {
            continue;
        };
        if target.is_empty() {
            continue;
        }
        let replacement = if *has_attribute_lines {
            let mut own_parts = Vec::new();
            while source[end..].starts_with('{') {
                let Some((length, own)) = read_djot_word_attributes(&source[end..], 0) else {
                    break;
                };
                own_parts.push(own);
                end += length;
            }
            let own_attrs = attributes(&own_parts.join(""));
            let mut attrs: Vec<_> = inherited.iter().collect();
            let mut slots: HashMap<&str, usize> = attrs
                .iter()
                .enumerate()
                .map(|(slot, (key, _))| (key.as_str(), slot))
                .collect();
            for own in &own_attrs {
                if let Some(&slot) = slots.get(own.0.as_str()) {
                    attrs[slot] = own;
                } else {
                    slots.insert(own.0.as_str(), attrs.len());
                    attrs.push(own);
                }
            }
            format!(
                "[{}][{}]{}",
                reference.text,
                aliases.get(&key).map(String::as_str).unwrap_or(&key),
                if attrs.is_empty() {
                    String::new()
                } else {
                    format!(
                        "{{{}}}",
                        attrs
                            .into_iter()
                            .flat_map(|(_, values)| values.iter())
                            .map(|value| {
                                if let Some((key, quoted)) = value.split_once("=\"") {
                                    let atom = quoted.strip_suffix('"').unwrap_or(quoted);
                                    if !atom.is_empty()
                                        && atom.chars().all(|ch| {
                                            ch.is_ascii_alphanumeric() || "_-.".contains(ch)
                                        })
                                    {
                                        return format!("{key}={atom}");
                                    }
                                }
                                value.to_owned()
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                }
            )
        } else if let Some(alias) = aliases.get(&key) {
            format!("[{}][{alias}]", reference.text)
        } else if (!reference.label.is_empty() && reference.label != key)
            || (reference.label.is_empty() && definition_label(reference.text) != key)
        {
            format!("[{}][{key}]", reference.text)
        } else {
            continue;
        };
        edits.push((start, end, replacement));
    }
    edits.sort_by_key(|edit| edit.0);
    let mut out = String::new();
    let mut cursor = 0;
    for (start, end, replacement) in edits {
        if start < cursor {
            continue;
        }
        out.push_str(&source[cursor..start]);
        out.push_str(&replacement);
        cursor = end;
    }
    out.push_str(&source[cursor..]);
    if out.starts_with('\n') && !source.starts_with('\n') {
        out = out.trim_start_matches('\n').to_owned();
    }
    if trim_definition_space && source.ends_with('\n') {
        out.truncate(out.trim_end_matches('\n').len());
        if !out.is_empty() {
            out.push('\n');
        }
    }
    if !moved_definitions.is_empty() {
        out = format!("{}\n\n{out}", moved_definitions.join("\n"));
    }
    out
}

fn reference_definitions(source: &str, mask: &str) -> (HashMap<String, String>, HashSet<usize>) {
    let definition = cached_regex!(r"^\[([^\[\]\n^][^\[\]\n]*)\]:[ \t]*(\S*)[ \t]*$").unwrap();
    let item = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]+").unwrap();
    let block =
        cached_regex!(r"^(?:#{1,6}(?: |$)|:{3,}|[`~]{3,}|\{|\[[^\]]+\]:|(?:[*-][ \t]*){3,}$)")
            .unwrap();
    let container = cached_regex!(r"(?:>|[-*+] |[0-9A-Za-z]+[.)] |\([0-9A-Za-z]+\) )").unwrap();
    let destination_token = cached_regex!(r"^\S+$").unwrap();
    let heading = cached_regex!(r"^(#{1,6})(?:[ \t]+|$)").unwrap();
    let heading_break = cached_regex!(r"^(?:[#>|{]|[-*+][ \t]|[0-9A-Za-z]+[.)][ \t]|:[ \t]|:{2,}|\([0-9a-zA-Z]+\)[ \t]|[`~]{3,}|\[[^\]\n]*\]:|(?:[*-][ \t]*){3,}$)").unwrap();
    let lines: Vec<_> = source.split('\n').collect();
    let masks: Vec<_> = mask.split('\n').collect();
    let rows = djot_table_rows(source, mask);
    let mut definitions = HashMap::new();
    let mut definition_lines = HashSet::new();
    let mut heading_lines = HashSet::new();
    for (n, line) in lines.iter().enumerate() {
        let at = djot_content_start(line);
        let content = &line[at..];
        let visible = &masks[n][at..];
        let previous = n.checked_sub(1).map_or("", |n| lines[n]);
        let previous_at = djot_content_start(previous);
        let previous_content = previous[previous_at..].trim();
        let boundary = previous_content.is_empty()
            || n > 0 && rows[n - 1]
            || item.is_match(&line[..at])
            || n > 0 && definition_lines.contains(&(n - 1))
            || block.is_match(previous_content)
            || previous_at > at && container.is_match(&previous[..previous_at]);
        if let Some(caps) = definition
            .captures(content)
            .filter(|_| visible.starts_with('[') && boundary)
        {
            let mut destination = caps[2].to_owned();
            for (next, line) in lines.iter().enumerate().skip(n + 1) {
                let next_at = djot_content_start(line);
                if next_at <= at || !destination_token.is_match(&line[next_at..]) {
                    break;
                }
                destination.push_str(&line[next_at..]);
                definition_lines.insert(next);
            }
            definitions.insert(definition_label(&caps[1]), destination);
            definition_lines.insert(n);
        }
        if let Some(heading) = heading
            .captures(visible)
            .filter(|_| boundary && !heading_lines.contains(&n))
        {
            let mut text = content[heading.get(0).unwrap().end()..].to_owned();
            let marker = regex::Regex::new(&format!(r"^{}[ \t]+", &heading[1])).unwrap();
            for (next, following) in lines.iter().enumerate().skip(n + 1) {
                let start = djot_content_start(following);
                let prefix = &following[..start];
                if prefix.bytes().filter(|b| *b == b'>').count()
                    != line[..at].bytes().filter(|b| *b == b'>').count()
                    || item.is_match(prefix)
                {
                    break;
                }
                let part = &following[start..];
                if part.trim().is_empty()
                    || text.bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 1
                {
                    break;
                }
                if let Some(m) = marker.find(part) {
                    text.push('\n');
                    text.push_str(&part[m.end()..]);
                } else {
                    if heading_break.is_match(part) {
                        break;
                    }
                    text.push('\n');
                    text.push_str(part);
                }
                heading_lines.insert(next);
            }
            definitions
                .entry(reference_label(&text, ""))
                .or_insert_with(|| "#heading".to_owned());
        }
    }
    (definitions, definition_lines)
}

fn loss_references(source: &str, mask: &str) -> HashMap<String, String> {
    reference_definitions(source, mask).0
}

fn empty_description(lines: &[&str], masks: &[&str], n: usize) -> bool {
    let term_pattern = cached_regex!(r":[ \t]+$").unwrap();
    let item_pattern = cached_regex!(r"(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]+").unwrap();
    let sibling_pattern =
        cached_regex!(r"^[ \t]*(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\)|:)[ \t]+").unwrap();
    let line = lines[n];
    let at = djot_content_start(line);
    let Some(term) = term_pattern.find(&masks[n][..at]) else {
        return false;
    };
    let previous = n.checked_sub(1).map_or("", |n| lines[n]);
    let previous_content = previous[djot_content_start(previous)..].trim();
    let item = item_pattern.is_match(&line[..at]);
    if term.start() > 0 && !masks[n].as_bytes()[term.start() - 1].is_ascii_whitespace()
        || !masks[n][at..]
            .chars()
            .next()
            .is_some_and(|ch| !ch.is_whitespace())
        || !(previous_content.is_empty() || n == 0 || item)
    {
        return false;
    }
    let depth = line[..at].bytes().filter(|b| *b == b'>').count();
    let without_quotes = |line: &str| -> String {
        let prefix = &line[..djot_content_start(line)];
        prefix
            .rfind('>')
            .map_or(line, |at| {
                line[at + 1..].strip_prefix(' ').unwrap_or(&line[at + 1..])
            })
            .to_owned()
    };
    let minimum = without_quotes(&line[..term.start()]).len() + 2;
    let mut end = n + 1;
    while end < lines.len() {
        let next = lines[end];
        let next_at = djot_content_start(next);
        let content = without_quotes(next);
        if next[..next_at].bytes().filter(|b| *b == b'>').count() != depth
            || next[next_at..].trim().is_empty()
            || sibling_pattern.is_match(&content) && leading_indent(&content).0 <= minimum - 2
        {
            break;
        }
        end += 1;
    }
    while end < lines.len()
        && lines[end][djot_content_start(lines[end])..]
            .trim()
            .is_empty()
    {
        end += 1;
    }
    end == lines.len()
        || lines[end][..djot_content_start(lines[end])]
            .bytes()
            .filter(|b| *b == b'>')
            .count()
            != depth
        || leading_indent(&without_quotes(lines[end])).0 < minimum
}

pub(super) fn losses(source: &str) -> Vec<crate::MigrationDiagnostic> {
    let empty_heading = cached_regex!(r"^#{1,6}$").unwrap();
    let heading_boundary = cached_regex!(r"^#{1,6}(?:[ \t]|$)").unwrap();

    use crate::{HtmlImportSeverity, MigrationConfidence, MigrationDiagnostic, MigrationFidelity};
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let (frontmatter, front_separator, source) = split_frontmatter(&normalized);
    let line_offset = frontmatter
        .bytes()
        .chain(front_separator.bytes())
        .filter(|b| *b == b'\n')
        .count();
    let mask = reference_mask(source);
    let mut findings: Vec<(usize, &'static str)> = Vec::new();
    if has_djot_code_layout_candidates(source) {
        let mut raw_code = Vec::new();
        let _ = close_djot_table_code(source, Some(&mut |at| raw_code.push(at)));
        findings.extend(raw_code.into_iter().map(|at| {
            (
                at,
                "Multiline inline code uses raw HTML; plain-text export omits it.",
            )
        }));
    }
    let mut nested = Vec::new();
    emphasis::convert_with_losses(source, &emphasis_mask(source), str::to_owned, &mut nested);
    findings.extend(nested.into_iter().map(|at| {
        (
            at,
            "Emphasis exceeding the native nesting budget is flattened; its text is preserved.",
        )
    }));
    let definitions = loss_references(source, &mask);
    let references = reference_uses(source, &mask);
    let mut label_cache = HashMap::new();
    for reference in &references {
        let at = reference.start;
        if reference.text.starts_with('^') {
            continue;
        }
        let image =
            at > 0 && source.as_bytes()[at - 1] == b'!' && !is_escaped(source.as_bytes(), at - 1);
        let destination = if definitions.is_empty() {
            None
        } else {
            definitions.get(&cached_reference_label(
                reference.text,
                reference.label,
                &mut label_cache,
            ))
        };
        match destination {
            Some(destination) if destination.is_empty() => findings.push((
                at,
                if image {
                    "An image with an empty destination cannot be written in Carve."
                } else {
                    "A link with an empty destination cannot be written in Carve."
                },
            )),
            None => findings.push((
                at,
                if image {
                    "An unresolved Djot image reference loses its image element."
                } else {
                    "An unresolved Djot reference loses its link element."
                },
            )),
            _ => (),
        }
    }
    let destinations = djot_simple_destination_ranges(source).unwrap_or_else(|| {
        let destination_mask = mask_djot_forms_with_options(
            source,
            false,
            true,
            None,
            &[],
            OpaqueOptions {
                destinations: false,
                attribute_values: false,
                autolinks: false,
                ..OpaqueOptions::default()
            },
        );
        djot_destination_ranges(source, &destination_mask)
    });
    let reference_starts: HashSet<_> = references.iter().map(|reference| reference.start).collect();
    let quote_prefix = cached_regex!(r"^(?:[ \t]*>(?:[ \t]|$))*").unwrap();
    let mut line_offsets = Vec::new();
    let mut quote_depths = Vec::new();
    let mut layout_offset = 0;
    for line in source.split('\n') {
        line_offsets.push(layout_offset);
        layout_offset += line.len() + 1;
        quote_depths.push(
            quote_prefix
                .find(line)
                .unwrap()
                .as_str()
                .bytes()
                .filter(|&byte| byte == b'>')
                .count(),
        );
    }
    let mut source_line = 0;
    let boundaries = djot_inline_boundaries(source, &mask);
    let mut boundary = 0;
    // Track label brackets separately from destination parentheses.
    let mut brackets: Vec<(usize, bool, usize)> = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while line_offsets.get(source_line + 1).is_some_and(|at| *at <= i) {
            source_line += 1;
        }
        while boundaries.get(boundary).is_some_and(|at| *at <= i) {
            brackets.clear();
            boundary += 1;
        }
        if mask.as_bytes()[i] != bytes[i] {
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'[' {
            brackets.push((i, false, quote_depths[source_line]));
        }
        if bytes[i] == b']' {
            if let Some((start, nested, depth)) = brackets.pop() {
                let image = start > 0 && bytes[start - 1] == b'!' && !is_escaped(bytes, start - 1);
                let valid_form =
                    destinations.contains_key(&(i + 1)) || reference_starts.contains(&start);
                let is_link = !image && bytes.get(start + 1) != Some(&b'^') && valid_form;
                if bytes.get(start + 1) != Some(&b'^')
                    && destinations.get(&(i + 1)).is_some_and(|&end| {
                        djot_destination_lines(&source[i + 2..end - 1], depth).is_empty()
                    })
                {
                    findings.push((
                        start,
                        if image {
                            "An image with an empty destination cannot be written in Carve."
                        } else {
                            "A link with an empty destination cannot be written in Carve."
                        },
                    ));
                }
                if is_link && nested {
                    findings.push((start, "A link nested inside another link is flattened."));
                }
                if is_link || nested && !(image && valid_form) {
                    if let Some(parent) = brackets.last_mut() {
                        parent.1 = true;
                    }
                }
            }
        }
        i += 1;
    }
    let lines: Vec<_> = source.split('\n').collect();
    let masks: Vec<_> = mask.split('\n').collect();
    let rows = djot_table_rows(source, &mask);
    let mut offset = 0;
    let mut table_start = 0;
    for (n, line) in lines.iter().enumerate() {
        if !rows[n] {
            table_start = n + 1;
        }
        let text = line.trim();
        let visible = masks[n].trim();
        let block_allowed = n == 0
            || lines[n - 1].trim().is_empty()
            || heading_boundary.is_match(masks[n - 1].trim_start())
            || definition_term(masks[n - 1]).is_some();
        if block_allowed
            && empty_heading.is_match(visible)
            && lines
                .get(n + 1)
                .map_or(true, |line| line.trim_matches([' ', '\t']).is_empty())
        {
            findings.push((offset, "An empty Djot heading cannot be written in Carve."));
        }
        if empty_description(&lines, &masks, n) {
            findings.push((
                offset,
                "An empty definition description cannot be written in Carve.",
            ));
        }
        if rows[n] && text.len() >= 2 {
            if separator(text) {
                if n > table_start + 1 {
                    findings.push((offset, "A table separator inside the table loses the preceding header row and alignment change."));
                }
                if (n == 0 || !rows[n - 1])
                    && lines
                        .iter()
                        .enumerate()
                        .skip(n)
                        .take_while(|(k, _)| rows[*k])
                        .all(|(_, row)| separator(row.trim()))
                {
                    findings.push((
                        offset,
                        "A table containing only separator rows cannot be written in Carve.",
                    ));
                }
            } else if text.trim_matches('|').trim().is_empty()
                && (n == 0 || !rows[n - 1])
                && rows.get(n + 1) != Some(&true)
            {
                findings.push((
                    offset,
                    "A table containing only blank cells cannot be written in Carve.",
                ));
            }
        }
        offset += line.len() + 1;
    }
    findings.sort_unstable();
    findings.dedup();
    let mut reported = HashSet::new();
    let mut diagnostics = Vec::new();
    let mut line = line_offset + 1;
    let mut cursor = 0;
    for (at, message) in findings {
        line += source.as_bytes()[cursor..at]
            .iter()
            .filter(|&&b| b == b'\n')
            .count();
        cursor = at;
        if reported.insert((line, message)) {
            diagnostics.push(MigrationDiagnostic {
                code: "structure-unspellable".to_owned(),
                message: message.to_owned(),
                severity: HtmlImportSeverity::Warning,
                fidelity: MigrationFidelity::Dropped,
                confidence: MigrationConfidence::Exact,
                path: Some(format!("line:{line}")),
            });
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use crate::{migrate_djot, HtmlImportSeverity, MigrationConfidence, MigrationFidelity};

    #[test]
    fn djot_empty_attributes_keep_bracketed_spans() {
        for tail in [
            "",
            "tail",
            "[x]",
            "[x](u)",
            "![x](u)",
            "*s*",
            "_e_",
            "`c`",
            "{=m=}",
            "{-d-}",
            "^u^",
            "~d~",
            "<http://a.b>",
            "word",
            "'q'",
            "\\*",
            "\\{",
        ] {
            let source = format!("[x]{{}}{tail}\n");
            let converted = migrate_djot(&source).value;
            assert!(converted.contains("[x]{}"), "{source:?}: {converted:?}");
            let tail_html = crate::to_html(&migrate_djot(&format!("{tail}\n")).value);
            let content = tail_html
                .strip_prefix("<p>")
                .and_then(|s| s.strip_suffix("</p>"))
                .unwrap_or(&tail_html);
            let content = if tail == "'q'" { "’q’" } else { content };
            assert_eq!(
                crate::to_html(&converted),
                format!("<p><span>x</span>{content}</p>"),
                "{source:?}"
            );
        }
        for (source, expected_html) in [
            ("pre [x]{} post\n", "<p>pre <span>x</span> post</p>"),
            ("*[x]{}*\n", "<p><strong><span>x</span></strong></p>"),
            ("_[x]{}_\n", "<p><em><span>x</span></em></p>"),
            ("[x]{}{}\n", "<p><span>x</span></p>"),
            ("![x]{}\n", "<p>!<span>x</span></p>"),
        ] {
            assert_eq!(
                crate::to_html(&migrate_djot(source).value),
                expected_html,
                "{source:?}"
            );
        }
    }

    #[test]
    fn djot_empty_attributes_without_spans_remain_inert() {
        for (source, expected) in [
            ("\\[x]{}\n", "\\[x]{%%}\n"),
            ("[x\\]{}\n", "[x\\]{%%}\n"),
            ("]{}\n", "]{%%}\n"),
            ("word{}\n", "word{%%}\n"),
            ("[x](u){}\n", "[x](u){%%}\n"),
            ("*a [b* c]{}\n", "*a \\[b* c\\]{%%}\n"),
            ("_a [b_ c]{}\n", "/a \\[b/ c\\]{%%}\n"),
        ] {
            assert_eq!(migrate_djot(source).value, expected, "{source:?}");
        }
    }

    #[test]
    fn djot_hard_breaks_preserve_escaped_spaces() {
        for prefix in ["a", "*a*"] {
            for spaces in ["\\ ", " \\ ", "\\ \\ "] {
                let source = format!("{prefix}{spaces}\\\nb\n");
                assert_eq!(super::inline(&source), source);
                assert_eq!(migrate_djot(&source).value, source);
                let content = if prefix == "a" {
                    "a"
                } else {
                    "<strong>a</strong>"
                };
                let spaces = spaces.replace("\\ ", "&nbsp;");
                assert_eq!(
                    crate::to_html(&source),
                    format!("<p>{content}{spaces}<br>\nb</p>")
                );
            }
        }
        for (source, expected) in [
            ("a\\ \\ \n", "a\\ \\\n"),
            ("a\\  \t\\\nb\n", "a\\ \\\nb\n"),
            ("a\\\\ \t\\\nb\n", "a\\\\\\\nb\n"),
            ("a  \t\\\nb\n", "a\\\nb\n"),
        ] {
            assert_eq!(super::inline(source), expected, "{source:?}");
            assert_eq!(
                crate::to_html(&migrate_djot(source).value),
                crate::to_html(expected),
                "{source:?}"
            );
        }
    }

    #[test]
    fn djot_block_after_quote_ends_the_container_paragraph() {
        let source = "> a\n# a\n";
        let value = migrate_djot(source).value;
        assert_eq!(value, source);
        assert_eq!(
            crate::to_html(&value),
            "<blockquote><p>a</p></blockquote>\n<section id=\"a\">\n  <h1>a</h1>\n</section>"
        );
    }

    #[test]
    fn djot_list_after_quote_ends_the_container_paragraph() {
        let value = migrate_djot("> a\n- *x*").value;
        assert_eq!(
            crate::to_html(&value),
            "<blockquote><p>a</p></blockquote>\n<ul>\n  <li><strong>x</strong></li>\n</ul>"
        );
        let value = migrate_djot("> a\n> > b").value;
        assert_eq!(
            crate::to_html(&value),
            "<blockquote><p>a\n&gt; b</p></blockquote>"
        );
        let value = migrate_djot("> > a\n> - *x*").value;
        assert_eq!(crate::to_html(&value), "<blockquote>\n  <blockquote><p>a</p></blockquote>\n  <ul>\n    <li><strong>x</strong></li>\n  </ul>\n</blockquote>");
        let value = migrate_djot("> > a\n>  - *x*").value;
        assert_eq!(crate::to_html(&value), "<blockquote>\n  <blockquote><p>a</p></blockquote>\n  <ul>\n    <li><strong>x</strong></li>\n  </ul>\n</blockquote>");
        let value = migrate_djot("> > a\n> b\n> > c").value;
        assert_eq!(
            crate::to_html(&value),
            "<blockquote>\n  <blockquote><p>a\nb\nc</p></blockquote>\n</blockquote>"
        );
    }

    #[test]
    fn djot_div_after_heading_continuation_starts_a_block() {
        let value = migrate_djot("# a\n  body\n::: x\n").value;
        assert_eq!(value, "# a body\n::: x\n:::\n");
        assert_eq!(
            crate::to_html(&value),
            "<section id=\"a-body\">\n  <h1>a body</h1>\n  <div class=\"x\">\n\n  </div>\n</section>"
        );
    }

    #[test]
    fn djot_empty_attributes_preserve_separate_emphasis_spans() {
        for (source, expected) in [
            ("*a*{}*b*\n", "*a*{%%}*b*\n"),
            ("_a_{}{}_b_\n", "/a/{%%}/b/\n"),
            ("*a*{ }*b*\n", "*a*{%%}*b*\n"),
        ] {
            let value = migrate_djot(source).value;
            assert_eq!(value, expected);
            let html = crate::to_html(&value);
            let tag = if source.starts_with('_') {
                "em"
            } else {
                "strong"
            };
            assert_eq!(
                html.matches(&format!("<{tag}>")).count(),
                2,
                "{source:?}: {html}"
            );
        }
        for (source, expected) in [
            ("~a~{}~b~\n", "{,a,}{%%}{,b,}\n"),
            ("^a^{}{}^b^\n", "{^a^}{%%}{^b^}\n"),
            ("{}{}\n*p*\n", "{%%}\n*p*\n"),
        ] {
            assert_eq!(migrate_djot(source).value, expected, "{source:?}");
        }
    }

    #[test]
    fn djot_list_blocks_preserve_looseness_and_parent_boundaries() {
        for source in [
            "- a\n\n  # h\n\n  - c\n\n- d\n",
            "- a\n\n  ```\n  x\n  ```\n\n  - c\n\n- d\n",
        ] {
            assert_eq!(migrate_djot(source).value, source, "{source:?}");
        }
        assert_eq!(
            migrate_djot("- a\n  para text\n\n  - c\n\n- d\n").value,
            "- a\n  para text\n  - c\n- d\n"
        );
        assert_eq!(
            migrate_djot("- a\n\n  - c\n  ```\n  x\n  ```\n\n- d\n").value,
            "- a\n\n  - c\n  ```\n  x\n  ```\n\n- d\n"
        );
    }

    #[test]
    fn djot_unspellable_structures_name_losses_on_source_lines() {
        for (source, line, message) in [
            ("##\n", 1, "empty Djot heading"),
            ("[link][]\n\n[link]:\n[link2]: url\n", 1, "empty destination"),
            ("[link][a and\nb]\n", 1, "unresolved Djot reference"),
            ("[link][a and\nb]\n\n[a and\nb]: url\n", 1, "unresolved Djot reference"),
            ("[Link][]\n\n[link]: /url\n", 1, "unresolved Djot reference"),
            ("[[foo](bar)](baz)\n", 1, "nested inside another link"),
            (": apple\n fruit\n\n  Paragraph one\n\n  Paragraph two\n\n  - sub\n  - list\n\n: orange\n", 11, "empty definition description"),
            ("|a|b|\n|:-|---:|\n|c|d|\n|cc|dd|\n|-:|:-:|\n|e|f|\n|g|h|\n", 5, "separator inside the table"),
            ("|--|--|\n", 1, "only separator rows"),
            ("| |\n", 1, "only blank cells"),
        ] {
            // Heading folding removes a source line before the loss location.
            for (prefix, shift) in [("", 0), ("# Heading\nlazy continuation\n\n", 3),
                ("# Heading\r\nlazy continuation\r\n\r\n", 3),
                ("---\ntext: *****a*****\n---\n\n", 4)] {
                let result = migrate_djot(&format!("{prefix}{source}"));
                assert!(result.report.diagnostics.iter().any(|d| d.code == "fidelity-unverified"));
                let loss = result.report.diagnostics.iter().find(|d| d.code == "structure-unspellable" && d.message.contains(message)).unwrap_or_else(|| panic!("{source:?}: {:?}", result.report.diagnostics));
                assert_eq!(loss.path.as_deref(), Some(format!("line:{}", line + shift).as_str()), "{source:?}");
                assert_eq!(loss.severity, HtmlImportSeverity::Warning);
                assert_eq!(loss.fidelity, MigrationFidelity::Dropped);
                assert_eq!(loss.confidence, MigrationConfidence::Exact);
            }
        }
    }

    #[test]
    fn djot_loss_detection_ignores_literal_and_supported_forms() {
        for source in [
            "`_({_foo_})_`\n",
            "|\n",
            "para\n#\n",
            "para\n: x\n",
            "[a][]\n\n[a]:\n  /url\n",
            "- [a][]\n\n  [a]: /url\n",
            "> [a][]\n>\n> [a]: /url\n",
            "See [Introduction][].\n\n# Introduction\n",
            "[![badge](b.svg)](https://x)\n",
            "---\ntext: *****a*****\n---\nplain\n",
            "[link _and_ link][]\n\n[link and link]: url\n",
            "```\n##\n[missing][]\n|--|--|\n```\n",
            "\\[missing][]\n",
            "[link][ref]\n\n[ref]: url\n",
            "# Heading\n",
            "*strong _emphasis_*\n",
            "|a|b|\n|--|--|\n",
        ] {
            assert!(
                migrate_djot(source)
                    .report
                    .diagnostics
                    .iter()
                    .all(|d| d.code != "structure-unspellable"),
                "{source:?}"
            );
        }
    }

    #[test]
    fn djot_normalization_preserves_code_urls_and_container_ownership() {
        for source in [
            "~~~\nlet s = ` a `;\n~~~\n",
            "<https://example.com/a{}b>\n",
            "[r]: /a{}b\n",
        ] {
            assert_eq!(super::inline(source), source);
        }
        assert!(super::inline("a `b\n\n{'q'} c\n\nd `e\n").contains("‘q’ c"));
        for (source, expected) in [
            ("- item\n  Note. more\n", "- item\n  Note. more\n"),
            ("- a\n  1. b\n", "- a\n  1\\. b\n"),
            ("a `b\nc`\n# f\n", "a `b\nc`\n\\# f\n"),
            ("para\n[a]: b\n# x\n", "para\n[a]: b\n\\# x\n"),
            (
                "1. one\n\n 1. two\n\n    cont\n",
                "1. one\n\n   1. two\n\n      cont\n",
            ),
            (
                ":::: outer\n::::: inner\nx\n::::\nafter\n",
                ":::: outer\n::::: inner\nx\n:::::\n::::\nafter\n",
            ),
            (
                "::: widget\n```\ncode\n",
                "::: widget\n```\ncode\n```\n:::\n",
            ),
        ] {
            assert_eq!(super::blocks(source), expected, "{source:?}");
        }
        let classes = migrate_djot("{.a .b}\n[ref]: /url\n\n[ref][]\n");
        assert!(classes.value.contains("{.a .b}"));
        let image = migrate_djot("{title=t}\n[ref]: /url\n\n[![alt](i.png)][ref]\n");
        assert_eq!(
            crate::to_html(&image.value),
            "<p><a href=\"/url\" title=\"t\"><img src=\"i.png\" alt=\"alt\"></a></p>"
        );
        assert_eq!(image.value.matches("/url").count(), 1);
        assert!(image.value.contains("[![alt](i.png)][ref]{title=\"t\"}"));
        assert_eq!(
            crate::to_html(&migrate_djot("<https://example.com/a{}b>\n").value),
            "<p><a href=\"https://example.com/a{}b\">https://example.com/a{}b</a></p>"
        );
    }

    #[test]
    fn djot_definition_term_lazy_continuation_is_dedented() {
        let result = migrate_djot(": apple\n fruit\n\n  Body\n\n: orange\n");
        assert!(result.value.contains(":: apple\nfruit\n"));
        assert_eq!(
            migrate_djot(": apple\n fruit\n\n  Body\n\n  - sub\n  - list\n\n: orange\n").value,
            "{loose}\n:: apple\nfruit\n\n:  Body\n\n   - sub\n   - list\n\n:: orange\n"
        );
    }

    #[test]
    fn djot_opaque_payloads_do_not_report_reference_losses() {
        for source in [
            "<https://example.com/[x][missing]>\n",
            "[x](a(b[x][missing]c))\n",
            "![x](a(b[x][missing]c))\n",
            "`[x][missing]`\n",
            "`[x][missing]`{=html}\n",
            "$`[x][missing]`\n",
            "[t]{k=\"[x][missing]\"}\n",
            "{% [x][missing] %}\n",
        ] {
            assert!(super::losses(source).is_empty(), "{source:?}");
        }
        for label in ["[x]", "![x]"] {
            assert_eq!(
                migrate_djot(&format!("{label}(a(b{{}}c))\n")).value,
                format!("{label}(a%28b{{}}c%29)\n")
            );
            assert_eq!(
                migrate_djot(&format!("{label}(a` b `c)\n")).value,
                format!("{label}(a%60%20b%20%60c)\n")
            );
            assert_eq!(
                migrate_djot(&format!("{label}(a` b `c) and ` code `\n")).value,
                format!("{label}(a%60%20b%20%60c) and `  code  `\n")
            );
        }
        for source in [
            "[x](a(` b `)c)\n",
            "[x](a` b\nc `d)\n",
            "`` x ``{=html}\n",
            "$`` x ``\n",
            "{% ` x ` %}\n",
        ] {
            assert_eq!(super::inline(source), source);
        }
        assert_eq!(super::inline("text](a` b `c)\n"), "text](a`  b  `c)\n");
        assert_eq!(super::inline("[` a](b `](url)\n"), "[`  a](b  `](url)\n");
        for destination in ["http://a.b/x{}y", "http://a.b/{\"q\"}"] {
            let source = format!("<{destination}>\n");
            let escaped = destination.replace('"', "&quot;");
            assert_eq!(
                crate::to_html(&migrate_djot(&source).value),
                format!("<p><a href=\"{escaped}\">{destination}</a></p>")
            );
        }
        let source = "{title=foo}\n[x]: /u\n\n[hi]{title=\"[x][]\"}\n";
        assert_eq!(
            crate::to_html(&migrate_djot(source).value),
            "<p><span title=\"[x][]\">hi</span></p>"
        );
    }

    #[test]
    fn djot_reference_attribute_pipes_remain_cell_boundaries() {
        let source = "{.c}\n[x]: /u\n\n| h |\n|---|\n| [a{k=\"a|b\"}][x] |\n";
        let normalized = super::references(source);
        assert!(normalized.contains("[a{k=\"a|b\"}][x]"));
        assert!(!normalized.contains("[x]{.c}"));
    }

    #[test]
    fn djot_reference_attributes_use_parsed_tokens_and_own_keys() {
        for (base, own, expected) in [
            ("{.a % .bogus #bogus title=bogus %}", "", "<p><a href=\"/u\" class=\"a\">x</a></p>"),
            ("{.a title=\"a .bogus #bogus % comment %\" key=\"a b\"}", "", "<p><a href=\"/u\" class=\"a\" title=\"a .bogus #bogus % comment %\" key=\"a b\">x</a></p>"),
            ("{.a title=base key=\"a b\"}", "{.b % .bogus key=bogus % title=\"own .c #d % value %\"}", "<p><a href=\"/u\" class=\"b\" title=\"own .c #d % value %\" key=\"a b\">x</a></p>"),
            ("{.a title=\"a \\\"quoted\\\" .b #c %\"}", "", "<p><a href=\"/u\" class=\"a\" title=\"a &quot;quoted&quot; .b #c %\">x</a></p>"),
            ("{.a class=b .c}", "", "<p><a href=\"/u\" class=\"b c\">x</a></p>"),
            ("{% note %}", "", "<p><a href=\"/u\">x</a></p>"),
            ("{}", "", "<p><a href=\"/u\">x</a></p>"),
            ("{.a}\n{.b}", "", "<p><a href=\"/u\" class=\"a b\">x</a></p>"),
            ("{key=a}\n{key=b}", "", "<p><a href=\"/u\" key=\"b\">x</a></p>"),
            ("{.a title=base}\n{.b key=value}", "{.c}{.d title=own}", "<p><a href=\"/u\" class=\"c d\" title=\"own\" key=\"value\">x</a></p>"),
        ] {
            let source = format!("[x][]{own}\n\n{base}\n[x]: /u\n");
            assert_eq!(crate::to_html(&migrate_djot(&source).value), expected, "{source:?}");
        }
        let source = "{.a #a title=a data-extra=x}\n[x]: /u\n\n[x][]{.b #b title=b}\n";
        let html = crate::to_html(&migrate_djot(source).value);
        for attribute in ["class=\"b\"", "id=\"b\"", "title=\"b\"", "data-extra=\"x\""] {
            assert!(html.contains(attribute), "{html}");
        }
    }

    #[test]
    fn djot_reference_attribute_runs_stop_at_blank_lines() {
        for source in [
            "{.a}\n\n[r]: u\n\n[x][r]\n",
            "{.a}\n{.b}\n\n[r]: u\n\n[x][r]\n",
        ] {
            let converted = migrate_djot(source).value;
            assert_eq!(
                crate::to_html(&converted),
                "<p><a href=\"u\">x</a></p>",
                "{converted:?}"
            );
        }
        assert_eq!(
            crate::to_html(&migrate_djot("{.a}\n\n{.b}\n[r]: u\n\n[x][r]\n").value),
            "<p><a href=\"u\" class=\"b\">x</a></p>"
        );
    }

    #[test]
    fn djot_formatted_reference_uses_share_definition_removal() {
        assert_eq!(
            crate::to_html(&migrate_djot("[_x_][] [_x_][]\n\n[x]: u\n").value),
            "<p><a href=\"u\"><em>x</em></a> <a href=\"u\"><em>x</em></a></p>"
        );
    }

    #[test]
    fn djot_div_closes_inside_its_owning_item() {
        let sibling_pattern = cached_regex!(r"</div>\s*</li>\s*<li>(?:<p>)?y").unwrap();
        for marker in ["-", "*", "1."] {
            let indent = if marker == "1." { "   " } else { "  " };
            let next = if marker == "1." { "2." } else { marker };
            for gap in ["", "\n", "\n\n"] {
                let source = format!("{marker} ::: foo\n{indent}x\n{gap}{next} y\n");
                let value = migrate_djot(&source).value;
                assert!(
                    value.contains(&format!("{indent}x\n{indent}:::\n{gap}{next} y")),
                    "{source:?}: {value:?}"
                );
                let html = crate::to_html(&value);
                assert_eq!(html.matches("<div").count(), 1);
                assert!(sibling_pattern.is_match(&html), "{html}");
            }
        }
        assert!(migrate_djot("- a\n\n  ::: foo\n  x\n- y\n")
            .value
            .contains("  x\n  :::\n- y\n"));
        let value = migrate_djot("- ::: foo\n  Hi\n  ::::").value;
        assert!(value.contains("\n  :::"));
        assert!(!crate::to_html(&value).contains("::::"));
    }

    #[test]
    fn djot_loss_references_follow_blocks_and_complete_heading_labels() {
        for block in [
            "***",
            "*-*-*",
            "# h",
            "> q",
            "- li",
            "1. li",
            "```\nc\n```",
            "| t |",
            "::: d\nin\n:::",
        ] {
            for source in [
                format!("[r][]\n\n{block}\n[r]: /u\n"),
                format!("{block}\n[r]: /u\n\n[r][]\n"),
            ] {
                assert!(super::losses(&source).is_empty(), "{source:?}");
            }
        }
        for heading in [
            "# a\ncontinued",
            "## a\n## continued",
            "#\na\ncontinued",
            "# a\n  continued",
        ] {
            assert!(
                super::losses(&format!("[a continued][]\n\n{heading}\n")).is_empty(),
                "{heading:?}"
            );
            assert!(
                !super::losses(&format!("[a][]\n\n{heading}\n")).is_empty(),
                "{heading:?}"
            );
        }
        for source in [
            "[r][]\n\npara\n[r]: /u\n",
            "[continued][]\n\n# a\n# continued\n",
            "[a continued][]\n\n# a\n- continued\n",
        ] {
            assert!(
                super::losses(source).iter().any(
                    |d| d.path.as_deref() == Some("line:1") && d.message.contains("unresolved")
                ),
                "{source:?}"
            );
        }
        assert!(super::losses("[a][]\n\n# a\n- continued\n").is_empty());
    }

    #[test]
    fn djot_empty_definition_losses_follow_container_content_and_source_lines() {
        for (source, line) in [
            (": term\n", 1),
            ("> : term\n", 1),
            ("- : term\n", 1),
            ("intro\n\n> : term\n", 3),
            ("- first\n- : term\n", 2),
            ("> > : term\n", 1),
            ("> - : term\n", 1),
            ("- > : term\n", 1),
            ("1. : term\n", 1),
            ("> : term\n>\n> outside\n", 1),
            ("- : term\n\n- outside\n", 1),
            ("> : term\n\n  outside\n", 1),
            ("- : term\n- sibling\n\n    body\n", 1),
            ("- : term\n  - sibling\n\n    body\n", 1),
            ("> : term\n> - sibling\n>\n>   body\n", 1),
        ] {
            let losses = super::losses(source);
            let loss = losses
                .iter()
                .find(|d| d.message.contains("empty definition description"))
                .unwrap_or_else(|| panic!("{source:?}: {losses:?}"));
            assert_eq!(loss.path.as_deref(), Some(format!("line:{line}").as_str()));
            assert_eq!(loss.fidelity, MigrationFidelity::Dropped);
            assert_eq!(loss.confidence, MigrationConfidence::Exact);
            assert_eq!(loss.severity, HtmlImportSeverity::Warning);
        }
        for source in [
            "> : term\n>\n>   body\n",
            "> > : term\n> >\n> >   body\n",
            "> - : term\n>\n>     body\n",
            "- > : term\n  >\n  >   body\n",
            "- : term\n\n    body\n",
            "1. : term\n\n     body\n",
            "- : term\n\n    - nested\n",
            ": term\n - continuation\n\n  body\n",
            "> paragraph\n> : term\n",
            "- paragraph\n  : term\n",
            "> ```\n> : term\n> ```\n",
            "- ```\n  : term\n  ```\n",
            "[^note]: body\n",
            "> [^note]: body\n",
            "- [^note]: body\n",
        ] {
            assert!(super::losses(source).is_empty(), "{source:?}");
        }
    }

    #[test]
    fn djot_ordered_lazy_markers_remain_in_the_open_paragraph() {
        for block in ["# h", "> q", "| t |", "::: n\n   in\n   :::", "***"] {
            let source = format!("1. a\n   {block}\n   1. c\n2. d\n");
            assert!(
                migrate_djot(&source).value.contains("   1\\. c"),
                "{source:?}"
            );
        }
        for marker in ["-", "*"] {
            let source = format!("{marker} a\n  {marker} c\n\n  # h\n{marker} d\n");
            assert!(crate::to_html(&migrate_djot(&source).value).contains("<li><p>d</p></li>"));
        }
        let source = "- a\n\n  second\n\n  - nested\n\n- d\n";
        assert!(crate::to_html(&migrate_djot(source).value).contains("<li><p>d</p></li>"));
    }
    #[test]
    fn djot_nested_formatted_reference_labels_do_not_reenter_the_importer() {
        assert_eq!(super::reference_label("*a*", ""), "a");
        let mut source = "*a*".to_owned();
        for _ in 0..32 {
            source = format!("[{source}][]");
        }
        let result = migrate_djot(&source);
        assert!(result.value.contains("*a*"));
        assert!(result
            .report
            .diagnostics
            .iter()
            .any(|d| d.code == "structure-unspellable"));
    }
    #[test]
    fn empty_heading_report_paths_follow_original_lines() {
        let source = "#\n\n".repeat(512);
        let result = migrate_djot(&source);
        let losses: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "structure-unspellable")
            .collect();
        assert_eq!(losses.len(), 512);
        assert_eq!(losses[0].path.as_deref(), Some("line:1"));
        assert_eq!(losses[511].path.as_deref(), Some("line:1023"));
    }

    #[test]
    fn unfinished_destination_report_is_literal() {
        let source = "x^[[a](b ".repeat(8192);
        let result = migrate_djot(&source);
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "structure-unspellable"));
    }
    #[test]
    fn an_unclosed_outer_destination_is_not_a_nested_link() {
        let result = migrate_djot("[[a](u)]( unfinished\n\nx)");
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("nested inside another link")));
    }
    #[test]
    fn nested_link_loss_survives_a_plain_bracket() {
        for source in [
            "[a [b [c](u)] d](v)",
            "[text [span [x](u)]{.c}](v)",
            "[![a [b](u)] text](v)",
            "[[a][r]](v)\n\n[r]: u",
            "[[a](u(1))](v)",
        ] {
            let result = migrate_djot(source);
            assert!(
                result
                    .report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message.contains("nested inside another link")),
                "{source}"
            );
        }
    }

    #[test]
    fn image_alt_links_are_not_nested_anchors() {
        let result = migrate_djot("[![a [b](u)](image)](outer)");
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("nested inside another link")));
    }
    #[test]
    fn empty_destinations_fold_container_prefixes() {
        for source in ["> [a](\n> )", "- [a](\n  )", "[a](\n  )"] {
            let report = migrate_djot(source);
            assert!(report
                .report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("empty destination")));
        }
        for source in ["[a][b][c]\n\n[b]: /u", "[a][b]()\n\n[b]: /u"] {
            let report = migrate_djot(source);
            assert!(!report
                .report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "structure-unspellable"));
        }
    }

    #[test]
    fn unfinished_code_reference_label_is_literal() {
        let report = migrate_djot("[a][b `c] [x][y]` z");
        assert!(!report
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "structure-unspellable"));
    }

    #[test]
    fn adjacent_live_reference_after_footnote_is_reported() {
        let report = migrate_djot("text[^1][see][ref]");
        assert!(report
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unresolved Djot reference")));
        let report = migrate_djot("[a](\n)");
        assert!(report
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("empty destination")));
    }

    #[test]
    fn missing_image_destinations_have_exact_losses() {
        for source in ["![a]()", "![a][missing]", "![a][]\n\n[a]:"] {
            let report = migrate_djot(source);
            assert!(report
                .report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "structure-unspellable"
                    && diagnostic.message.contains("image")));
        }
        let report = migrate_djot("[a]( )");
        assert!(!report
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("empty destination")));
    }

    #[test]
    fn adjacent_footnotes_are_not_reference_links() {
        let report = migrate_djot("[see [^1][^2]](v)\n\n[^1]: one\n\n[^2]: two");
        assert!(!report
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "structure-unspellable"));
    }

    #[test]
    fn escaped_bang_keeps_unresolved_link_loss() {
        let report = migrate_djot("\\![x][undefined]");
        assert!(report
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unresolved Djot reference")));
    }

    #[test]
    fn unfinished_raw_formats_stop_at_line_boundaries() {
        let source = "`a`{=".repeat(8192);
        assert_eq!(
            super::mask_djot_opaque(&source, super::OpaqueOptions::default()),
            "   {=".repeat(8192).as_bytes()
        );
        assert_eq!(
            super::mask_djot_opaque(&(source + "\n}"), super::OpaqueOptions::default()),
            ("   {=".repeat(8192) + "\n}").as_bytes()
        );
    }

    #[test]
    fn inline_pseudo_definitions_stay_literal() {
        let source = "[] [a]: ".repeat(8192);
        assert_eq!(
            super::mask_djot_opaque(&source, super::OpaqueOptions::default()),
            source.as_bytes()
        );
        assert_eq!(
            super::mask_djot_opaque("[a]: u\n[b]: v", super::OpaqueOptions::default()),
            b"[a]:  \n[b]:  "
        );
    }

    #[test]
    fn repeated_unfinished_comments_stay_literal() {
        let source = "{% ".repeat(8192);
        let mask = super::mask_djot_opaque(&source, super::OpaqueOptions::default());
        assert_eq!(mask, source.as_bytes());
        assert_eq!(
            crate::to_html(&migrate_djot(&source).value),
            format!("<p>{}</p>", source.trim_end())
        );
    }
}
