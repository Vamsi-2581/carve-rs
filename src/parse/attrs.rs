use super::*;

pub(in crate::parse) fn parse_standalone_attrs(line: &str) -> Option<Attrs> {
    let trimmed = trim_ascii(line);
    if !trimmed.starts_with('{') {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let last_close_brace = bytes.iter().rposition(|&b| b == b'}');
    let mut pos = 0usize;
    let mut attrs: Option<Attrs> = None;
    let mut slots = None;
    while pos < bytes.len() {
        // The block-attribute LINE, so `whitespace` at all three of its slots.
        let (incoming, next) = read_attrs_at_with(bytes, pos, last_close_brace, false)?;
        merge_attrs_indexed(&mut attrs, incoming, &mut slots);
        pos = next;
        if pos < bytes.len() && bytes[pos] != b'{' {
            return None;
        }
    }
    attrs
}

#[cfg(test)]
thread_local! {
    pub(in crate::parse) static ATTRS_LOOKAHEAD_BYTES: Cell<usize> = const { Cell::new(0) };
}

pub(in crate::parse) fn valid_wrapped_attr_continuation(line: &str) -> bool {
    trim_ascii_end(line).ends_with('}') || parse_attrs_with(trim_ascii(line), false).is_some()
}

/// A standalone block-attribute block, possibly spanning several contiguous
/// (non-blank) lines: it opens with `{` and closes with `}` on a later line
/// (`{#id` / ` .foo}`). Consumes the lines and returns the parsed attributes,
/// or leaves the cursor untouched if it is not a valid attribute block.
pub(in crate::parse) fn parse_standalone_attrs_block(cur: &mut LineCursor) -> Option<Attrs> {
    // The OPENER is read RAW: a framed first line is a lazily-folded indented
    // `{...}`, which is paragraph text under the strict column-0 rule and must
    // NOT be pulled flush-left into a block. Only the CONTINUATION lines below
    // are unframed, so a wrapped block whose CLOSER folded down from a shallower
    // quote depth still joins (carve-rs#1538); the opener there arrives unframed
    // through the quote's own marker.
    let first = cur.peek()?;
    if !trim_ascii_start(first).starts_with('{') {
        return None;
    }
    if let Some(attrs) = parse_standalone_attrs(first) {
        cur.consume();
        return Some(attrs);
    }
    // See `standalone_attrs_block_len` for the cursor-free twin of everything
    // below; the two must keep answering the same way.
    // A COMPLETE single line (already closes with `}`) that parse_standalone_attrs
    // rejected is not a valid attribute block -- do NOT rescue it via the
    // multi-line strip-outer path below, which would parse an interior `}{` as an
    // unquoted value (`{k=v}{+i+}` -> k="v}{+i+", swallowing the whole line). The
    // multi-line join is only for a block that genuinely continues onto later
    // lines (`{#id` then `.foo}`), i.e. whose first line does not itself close.
    // Matches carve-js, which keeps such a line literal.
    if trim_ascii_end(first).ends_with('}') {
        return None;
    }
    // Multi-line: join contiguous lines until one closes with `}`.
    let mut joined = String::new();
    let mut count = 0usize;
    let mut quote: Option<char> = None;
    while let Some(raw) = cur.lines.line(cur.pos + count) {
        let line = strip_lazy(raw);
        if is_blank_line(line) {
            return None;
        }
        if count > 0 && !valid_wrapped_attr_continuation(line) {
            return None;
        }
        if !joined.is_empty() {
            if quote.is_some() {
                return None;
            }
            joined.push(' ');
        }
        #[cfg(test)]
        ATTRS_LOOKAHEAD_BYTES.with(|work| work.set(work.get() + line.len()));
        joined.push_str(trim_ascii(line));
        quote = open_quote_at_end(trim_ascii(line));
        count += 1;
        if trim_ascii_end(line).ends_with('}') {
            let inner = trim_ascii(&joined);
            if inner.starts_with('{') && inner.ends_with('}') {
                if let Some(attrs) = parse_attrs_with(&inner[1..inner.len() - 1], false) {
                    for _ in 0..count {
                        cur.consume();
                    }
                    return Some(attrs);
                }
            }
            return None;
        }
    }
    None
}

/// How many LINES a standalone attribute block occupies when one starts at
/// `lines[0]`, or `None` when these lines do not spell one.
///
/// The cursor-free twin of `parse_standalone_attrs_block`, for the predicates
/// that must ask "does an attribute block sit here" without a cursor to consume
/// from. Both follow the same rules: a single complete line, or a run joined
/// until one closes with `}`, refused at a blank line and refused when a quoted
/// value is left open across a break (PART 4, markup-carve/carve#888).
///
/// FLUSH-LEFT ONLY, like every caller's own guard: an indented `{...}` is lazy
/// paragraph text under the strict column-0 rule (PART 9 section 24 C3).
pub(in crate::parse) fn standalone_attrs_block_len(lines: &[&str]) -> Option<usize> {
    standalone_attrs_block_len_iter(lines.iter().copied())
}

pub(in crate::parse) fn standalone_attrs_block_len_iter<'a>(
    mut lines: impl Iterator<Item = &'a str>,
) -> Option<usize> {
    // OPENER read RAW (see `parse_standalone_attrs_block`): a framed first line
    // is lazily-folded indented text, not a block opener. Only the CONTINUATION
    // lines in the join loop below are unframed.
    let first = lines.next()?;
    if first.starts_with([' ', '\t']) {
        return None;
    }
    // AN ATTRIBUTE BLOCK OPENS WITH A BRACE, and refusing everything else here
    // is what keeps this cheap. `interrupts_paragraph` asks this question of
    // EVERY continuation line, and without the guard a paragraph whose lines
    // never end in `}` sends the join loop below over the whole remainder once
    // per line - quadratic time and allocation on ordinary prose that has no
    // attributes in it at all.
    if !trim_ascii(first).starts_with('{') {
        return None;
    }
    if parse_standalone_attrs(first).is_some() {
        return Some(1);
    }
    // Same refusal as the cursor form: a line that already closes and was
    // rejected is not rescued by the multi-line join.
    if trim_ascii_end(first).ends_with('}') {
        return None;
    }
    let mut joined = String::new();
    let mut quote: Option<char> = None;
    for (count, raw) in std::iter::once(first).chain(lines).enumerate() {
        let line = strip_lazy(raw);
        if is_blank_line(line) {
            return None;
        }
        if count > 0 && !valid_wrapped_attr_continuation(line) {
            return None;
        }
        if !joined.is_empty() {
            if quote.is_some() {
                return None;
            }
            joined.push(' ');
        }
        #[cfg(test)]
        ATTRS_LOOKAHEAD_BYTES.with(|work| work.set(work.get() + line.len()));
        joined.push_str(trim_ascii(line));
        quote = open_quote_at_end(trim_ascii(line));
        if trim_ascii_end(line).ends_with('}') {
            let inner = trim_ascii(&joined);
            if inner.starts_with('{')
                && inner.ends_with('}')
                && parse_attrs_with(&inner[1..inner.len() - 1], false).is_some()
            {
                return Some(count + 1);
            }
            return None;
        }
    }
    None
}

/// What a quoted line says about a standalone attribute block starting on it.
pub(in crate::parse) enum QuotedAttrsBlock {
    /// One starts here and spans this many QUOTED lines.
    Block(usize),
    /// None starts here, and none can start on any of the next `0..n` lines
    /// either: the scan walked that far without meeting a line that could CLOSE
    /// one. See [`quoted_attrs_block_len`] for why that generalizes.
    NoneWithin(usize),
    /// None starts here. A later line may still open one.
    No,
}

/// Whether a standalone attribute block starts on the quoted line `stripped`,
/// and how many QUOTED lines it takes.
///
/// `stripped` is the current line with ONE quote marker taken off, and `rest`
/// is the raw document from the next line on. Both are walked down to their
/// INNERMOST quoted content, exactly as [`ParaOpen::resolve`] does: a lazy line
/// continues the innermost open paragraph, so what an attribute block ends is
/// the innermost paragraph too, and a block written at depth 2 has to answer
/// like one written at depth 1 (markup-carve/carve#506).
///
/// THE BRACE GUARD COMES FIRST AND IS TESTED ON THE UNWALKED LINE. Every quoted
/// line reaches this, and the walk costs one strip per remaining quote marker -
/// which is precisely the per-line cost `ParaOpen` was built to defer
/// (markup-carve/carve-rs#731). A line with no `{` anywhere on it cannot open an
/// attribute block at any depth, so a quote ladder pays one `memchr` and stops.
///
/// AND THE SCAN REPORTS WHAT IT PROVED, so a run of brace-shaped lines that
/// close nothing is walked ONCE rather than once per line. A block ends at the
/// first line with a trailing `}`; if the walk reaches a blank line or the end
/// of the quoted run without meeting one, no line it passed can open a block
/// either, and `NoneWithin` hands the caller that window to skip. Without it a
/// quoted `{a` repeated n times pays an O(n) scan per line. Invalid continuation
/// fragments also end the window before a later opener is skipped.
pub(in crate::parse) fn quoted_attrs_block_len<'a>(
    stripped: &'a str,
    rest: impl Iterator<Item = &'a str>,
) -> QuotedAttrsBlock {
    if !stripped.contains('{') {
        return QuotedAttrsBlock::No;
    }
    fn innermost(mut line: &str) -> &str {
        while let Some(next) = strip_blockquote_prefix(line) {
            line = next;
        }
        line
    }
    let first = innermost(stripped);
    if first.starts_with([' ', '\t']) || !first.starts_with('{') {
        return QuotedAttrsBlock::No;
    }
    // The single-line spelling is already answered by
    // `interrupts_paragraph_with_rest`, which `ParaOpen` consults. Only the
    // WRAPPED one needs the lookahead, and refusing a complete line here keeps
    // this from being a second answer to a question that already has one.
    if trim_ascii_end(first).ends_with('}') {
        return QuotedAttrsBlock::No;
    }
    let mut block: Vec<&str> = vec![first];
    let mut met_a_closer = false;
    for line in rest {
        // The block lives INSIDE the quote: an unprefixed line is the lazy
        // continuation the caller is deciding about, not part of the block.
        let Some(next) = strip_blockquote_prefix(line) else {
            break;
        };
        let inner = innermost(next);
        #[cfg(test)]
        ATTRS_LOOKAHEAD_BYTES.with(|work| work.set(work.get() + inner.len()));
        if !is_blank_line(inner) && !valid_wrapped_attr_continuation(inner) {
            return QuotedAttrsBlock::NoneWithin(block.len());
        }
        block.push(inner);
        if is_blank_line(inner) {
            break;
        }
        if trim_ascii_end(inner).ends_with('}') {
            met_a_closer = true;
            break;
        }
    }
    match standalone_attrs_block_len(&block) {
        Some(len) => QuotedAttrsBlock::Block(len),
        // A closer was there and the join was still refused - a later line may
        // open a block that ends on that same closer, so nothing is proved.
        None if met_a_closer => QuotedAttrsBlock::No,
        None => QuotedAttrsBlock::NoneWithin(block.len()),
    }
}

/// The quote a run leaves OPEN at its end.
///
/// A backslash escapes the next character, so `\"` neither opens a value nor
/// closes one. Used to decide whether a line break falls inside a
/// `quoted_value`, which PART 4 forbids in both of its alternatives
/// (carve#888).
///
/// The scan starts from NO open quote every time, and deliberately takes no
/// incoming state: a line that ends inside a value refuses the block at the
/// very next break, so no later line is ever scanned from inside one. A
/// carried-in state would be a parameter that cannot take a second value -
/// dead by construction and untestable, which is the shape this repository has
/// had to remove repeatedly.
pub(in crate::parse) fn open_quote_at_end(s: &str) -> Option<char> {
    let mut quote = None;
    let mut escaped = false;
    for ch in s.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some(q) => {
                if ch == '\\' {
                    escaped = true;
                } else if ch == q {
                    quote = None;
                }
            }
            None => match ch {
                '\\' => escaped = true,
                '"' | '\'' => quote = Some(ch),
                _ => {}
            },
        }
    }
    quote
}

/// Split a TRAILING standalone block-attribute block off a collected chunk,
/// returning its attributes and shortening the chunk to the lines before it.
pub(in crate::parse) fn split_trailing_attrs(
    nested: &mut MappedSource,
) -> Option<(Attrs, Option<Pos>)> {
    let lines: Vec<&str> = nested.source.lines().collect();
    if lines.is_empty() {
        return None;
    }
    // A trailing attribute block ends on the last line, so that line must close
    // one. Anything else is ordinary content and there is nothing to split.
    if !trim_ascii_end(lines[lines.len() - 1]).ends_with('}') {
        return None;
    }
    // Walk back to the line that OPENS the run. A blank ends the search: a
    // block-attribute block is contiguous.
    let mut start = lines.len() - 1;
    loop {
        let line = lines[start];
        if is_blank_line(line) {
            return None;
        }
        // FLUSH LEFT, the same guard `parse_blocks` puts on this call
        // (`line_flush`). The chunk is already dedented by the item's content
        // column, so flush here means AT that column - and a line PAST it is
        // ordinary text, which is what corpus 87-compact-list-blocks-10 pins:
        // `- a` / blank / three spaces / `{.c}` is a second paragraph reading
        // `{.c}`, not an attribute block. Trimming the indent away instead
        // deleted that paragraph and re-tightened the item.
        if line.starts_with('{') {
            break;
        }
        if line.starts_with([' ', '\t']) {
            return None;
        }
        if start == 0 {
            return None;
        }
        start -= 1;
    }
    let tail: Vec<&str> = lines[start..].to_vec();
    let mut probe = LineCursor::new_with_cols(&tail, None, None);
    let attrs = parse_standalone_attrs_block(&mut probe)?;
    // Only a run the reader consumed WHOLE is the chunk's trailing attribute
    // block. A partial consume means the lines after it are content, and
    // splitting there would silently delete them.
    if !probe.eof() {
        return None;
    }
    // WHERE THE RUN WAS WRITTEN, for §15 A4's diagnostic - taken BEFORE the
    // maps are truncated below, since after that the split-off lines have no
    // entries left to read. A set lifted off a chunk and never placed is
    // dropped when the list ends or when a sibling marker opens
    // (markup-carve/carve#1281), and the drop is a finding wherever it happens.
    let pos = mapped_span_of(nested, start, lines.len(), &lines);
    let line_count = lines.len();
    let kept: Vec<&str> = lines[..start].to_vec();
    let source = kept.join("\n");
    nested.source = source;
    // Both maps are read by LINE INDEX, and the kept lines keep the indices
    // they had, so trailing entries are simply never reached - trimming them is
    // tidiness, not correctness. It is only safe on a map that is index-aligned
    // with the chunk: `push_newline_at` skips a LEADING run of unmapped lines
    // rather than storing `None` for them, so a shorter `line_map` is offset
    // from the lines and cutting it at `start` would drop an entry belonging to
    // a line being KEPT. Leave such a map alone; its extra entries cost nothing.
    if nested.col_map.len() == line_count {
        nested.col_map.truncate(start);
    }
    if nested.line_map.len() == line_count {
        nested.line_map.truncate(start);
    }
    Some((attrs, pos))
}

/// The document span of `lines[start..end]` inside a collected chunk.
///
/// [`span_of`] answers this for a [`LineCursor`], which carries the same two
/// maps under different names; a chunk that has already been split off has no
/// cursor over it, so the same arithmetic is done here. `None` when the chunk
/// carries no line map - positions are opt-in, and a span nobody asked for is
/// not worth inventing.
pub(in crate::parse) fn mapped_span_of(
    nested: &MappedSource,
    start: usize,
    end: usize,
    lines: &[&str],
) -> Option<Pos> {
    let last = end.saturating_sub(1).max(start);
    let start_line = *nested.line_map.get(start)?;
    let start_line = start_line?;
    let end_line = nested
        .line_map
        .get(last)
        .copied()
        .flatten()
        .unwrap_or(start_line);
    let stripped = nested.col_map.get(start).copied().flatten().unwrap_or(0);
    let end_stripped = nested
        .col_map
        .get(last)
        .copied()
        .flatten()
        .unwrap_or(stripped);
    let indent = lines
        .get(start)
        .map(|l| l.chars().count() - trim_ascii_start(l).chars().count())
        .unwrap_or(0);
    let width = lines.get(last).map(|l| l.chars().count()).unwrap_or(0);
    Some(Pos {
        start_line,
        end_line,
        start_column: (stripped.max(0) as usize) + indent + 1,
        end_column: (end_stripped.max(0) as usize) + width + 1,
        start_offset: 0,
        end_offset: 0,
        file: None,
    })
}

pub(in crate::parse) fn merge_attrs(target: &mut Option<Attrs>, incoming: Attrs) {
    if target.is_none() {
        *target = Some(incoming);
        return;
    }
    merge_attrs_indexed(target, incoming, &mut None);
}

pub(in crate::parse) fn merge_attrs_indexed(
    target: &mut Option<Attrs>,
    incoming: Attrs,
    slots: &mut Option<HashSet<AttrSlot>>,
) {
    if target.is_none() {
        *slots = None;
        *target = Some(incoming);
        return;
    }
    let target = target.as_mut().unwrap();
    let slots = slots.get_or_insert_with(|| target.order.iter().cloned().collect());
    if incoming.id.is_some() {
        target.id = incoming.id;
    }
    target.classes.extend(incoming.classes);
    target.key_values.extend(incoming.key_values);
    for slot in incoming.order {
        if slots.insert(slot.clone()) {
            target.order.push(slot);
        }
    }
}

/// Merge a leading block-attribute line onto a node that may already carry
/// its own opener attributes. Leading attrs are earlier in source, so their
/// classes precede the opener's and the opener wins on id/key conflict (§15).
pub(crate) fn merge_leading_attrs(target: &mut Option<Attrs>, leading: Attrs) {
    match target.take() {
        None => *target = Some(leading),
        Some(own) => {
            *target = Some(leading);
            merge_attrs(target, own);
        }
    }
}

/// Add a `data-source-line` attribute to a block node, preserving any existing
/// attributes. No-op for blocks that carry no attributes (comment,
/// abbreviation definition).
pub(in crate::parse) fn stamp_source_line(node: &mut BlockNode, line: usize) {
    let slot: Option<&mut Option<Attrs>> = match node {
        // §15 A2b puts a definition's attributes on its own line, not from a
        // preceding block-attribute line, so there is no slot to fill here.
        BlockNode::LinkReferenceDefinition(_) => None,
        BlockNode::Heading(n) => Some(&mut n.attrs),
        BlockNode::Paragraph(n) => Some(&mut n.attrs),
        BlockNode::ThematicBreak(n) => Some(&mut n.attrs),
        BlockNode::CodeBlock(n) => Some(&mut n.attrs),
        BlockNode::RawBlock(n) => Some(&mut n.attrs),
        BlockNode::List(n) => Some(&mut n.attrs),
        BlockNode::BlockQuote(n) => Some(&mut n.attrs),
        BlockNode::Table(n) => Some(&mut n.attrs),
        BlockNode::Admonition(n) => Some(&mut n.attrs),
        BlockNode::Directive(n) => Some(&mut n.attrs),
        BlockNode::Div(n) => Some(&mut n.attrs),
        BlockNode::Section(n) => Some(&mut n.attrs),
        BlockNode::LineBlock(n) => Some(&mut n.attrs),
        BlockNode::DefinitionList(n) => Some(&mut n.attrs),
        BlockNode::Figure(n) => Some(&mut n.attrs),
        BlockNode::FigureGroup(n) => Some(&mut n.attrs),
        BlockNode::BlockExtension(n) => Some(&mut n.attrs),
        BlockNode::ExtensionCarrier(n) => Some(&mut n.attrs),
        BlockNode::BlockImage(n) => Some(&mut n.attrs),
        // A citation definition's attributes are the metadata block on its own
        // line, not a preceding block-attribute line - same as the link
        // reference definition above.
        BlockNode::AbbreviationDef(_)
        | BlockNode::CitationDefinition(_)
        | BlockNode::Comment(_) => None,
    };
    let Some(opt) = slot else {
        return;
    };
    let attrs = opt.get_or_insert_with(Attrs::default);
    stamp_source_line_attr(attrs, line);
}

pub(in crate::parse) fn source_line_attrs(
    mut attrs: Option<Attrs>,
    line: Option<usize>,
    options: &Options<'_>,
) -> Option<Attrs> {
    if options.source_lines {
        if let Some(line) = line {
            let attrs = attrs.get_or_insert_with(Attrs::default);
            stamp_source_line_attr(attrs, line);
        }
    }
    attrs
}

pub(in crate::parse) fn stamp_source_line_attr(attrs: &mut Attrs, line: usize) {
    let key = "data-source-line";
    if !attrs.key_values.contains_key(key) {
        attrs.key_values.insert(key.to_string(), line.to_string());
        attrs.order.push(AttrSlot::Key(key.to_string()));
    }
}

/// PART 9 §17 L7: the consumed `loose` boolean, taken off the attribute line.
///
/// Returns whether the key was SPELLED, and removes it, so it never reaches the
/// output as an HTML attribute. The precedent is PART 12 §15's `header-rows`:
/// a structural fact riding the block-attribute line, consumed rather than
/// emitted.
///
/// A BOOLEAN AND AN EMPTY VALUE ARE ONE KEY. PART 4 makes `{loose}` and
/// `{loose=""}` the same attribute, and both arrive here with an empty value,
/// so both are consumed. `loose=x` names a value this key does not take, so it
/// is NOT this key: it is left alone and renders `loose="x"` like any other
/// attribute. There is no error state and no spelling that half-applies.
pub(in crate::parse) fn consume_loose(attrs: &mut Attrs) -> bool {
    if attrs.key_values.get("loose").map(String::as_str) != Some("") {
        return false;
    }
    attrs.key_values.remove("loose");
    attrs
        .order
        .retain(|slot| !matches!(slot, AttrSlot::Key(key) if key == "loose"));
    true
}

pub(in crate::parse) fn attrs_are_empty(attrs: &Attrs) -> bool {
    attrs.id.is_none()
        && attrs.classes.is_empty()
        && attrs.key_values.is_empty()
        && attrs.order.is_empty()
}

pub(in crate::parse) fn apply_attrs_to_block(node: &mut BlockNode, attrs: Attrs) {
    let mut attrs = attrs;
    match node {
        BlockNode::Heading(n) => n.attrs = Some(attrs),
        BlockNode::Paragraph(n) => n.attrs = Some(attrs),
        BlockNode::ThematicBreak(n) => n.attrs = Some(attrs),
        BlockNode::CodeBlock(n) => n.attrs = Some(attrs),
        BlockNode::RawBlock(n) => n.attrs = Some(attrs),
        BlockNode::List(n) => {
            // §17 L7. The axis is TOTAL for a list, so the spelled fact lands in
            // the field that already states it and nothing else is published.
            // Redundant use is a legal no-op: a list the blank lines already
            // loosened is already `tight: false`.
            if consume_loose(&mut attrs) {
                n.tight = false;
                // A LINE THAT CARRIED ONLY THE KEY LEAVES NO ATTRIBUTES BEHIND.
                // Attaching the emptied set published `attrs: {}` on a node the
                // author gave no attributes, which is a tree the same document
                // written without the key does not produce - so `fmt` of a
                // one-item loose list stopped round-tripping to its own parse.
                if attrs_are_empty(&attrs) {
                    n.attrs = None;
                    return;
                }
            }
            n.attrs = Some(attrs);
        }
        BlockNode::BlockQuote(n) => n.attrs = Some(attrs),
        BlockNode::Table(n) => {
            n.columns = table_columns_from_attrs(&attrs);
            n.row_groups =
                crate::table_source_metadata::row_groups(&attrs, n.rows.len()).map(Box::new);
            n.attrs = Some(attrs);
        }
        // A typed colon-fence opener may already carry its own attribute
        // block (`::: note {.x}`); a leading block-attribute line is earlier
        // in source, so its classes come first and the opener's win on
        // id/key conflict (§15) -- merge instead of clobbering.
        BlockNode::Admonition(n) => merge_leading_attrs(&mut n.attrs, attrs),
        BlockNode::Directive(n) => merge_leading_attrs(&mut n.attrs, attrs),
        BlockNode::Div(n) => merge_leading_attrs(&mut n.attrs, attrs),
        BlockNode::LineBlock(n) => merge_leading_attrs(&mut n.attrs, attrs),
        BlockNode::DefinitionList(n) => {
            // §17 L7. The axis is NOT total here: a `<dl>` derives a loose
            // description from its block count and has no blank-line spelling
            // for a one-block one at any entry count, so the spelled fact needs
            // a field of its own rather than landing in one that already states
            // it. Redundant use stays a legal no-op - a description already
            // holding two blocks renders the same either way.
            if consume_loose(&mut attrs) {
                n.loose = true;
                // The same emptied-set rule the list arm applies: a line that
                // carried only the key leaves no attributes behind, so `fmt`
                // round-trips to its own parse.
                if attrs_are_empty(&attrs) {
                    n.attrs = None;
                    return;
                }
            }
            n.attrs = Some(attrs);
        }
        BlockNode::Figure(n) => n.attrs = Some(attrs),
        // The bare opener carries nothing of its own (§4c), so the preceding
        // block-attribute line is the group's only attribute source.
        BlockNode::FigureGroup(n) => n.attrs = Some(attrs),
        BlockNode::ExtensionCarrier(n) => n.attrs = Some(attrs),
        // A direct block image (`{#id}\n![…](…)`) carries the leading attrs on
        // the `<img>` itself; the image's own inline attrs win on conflict (§15).
        BlockNode::BlockImage(img) => merge_leading_attrs(&mut img.attrs, attrs),
        _ => {}
    }
}

pub(in crate::parse) fn table_columns_from_attrs(attrs: &Attrs) -> Vec<TableColumn> {
    let split = |key: &str| {
        attrs
            .key_values
            .get(key)
            .map(|v| v.split(',').collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let aligns = split("aligns");
    let valigns = split("valigns");
    let widths = split("widths");
    let len = aligns.len().max(valigns.len()).max(widths.len());
    (0..len)
        .map(|i| TableColumn {
            align: aligns.get(i).and_then(|v| match *v {
                "left" => Some(TableAlign::Left),
                "right" => Some(TableAlign::Right),
                "center" => Some(TableAlign::Center),
                _ => None,
            }),
            valign: valigns.get(i).and_then(|v| match *v {
                "top" => Some(TableVerticalAlign::Top),
                "middle" => Some(TableVerticalAlign::Middle),
                "bottom" => Some(TableVerticalAlign::Bottom),
                _ => None,
            }),
            width: widths.get(i).and_then(|v| crate::table_width::fraction(v)),
        })
        .collect()
}

pub(in crate::parse) fn apply_attrs_to_inline(node: &mut InlineNode, attrs: Attrs) {
    match node {
        InlineNode::Emphasis(n) => n.attrs = Some(attrs),
        InlineNode::Link(n) => n.attrs = Some(attrs),
        InlineNode::Image(n) => n.attrs = Some(attrs),
        InlineNode::Span(n) => n.attrs = Some(attrs),
        InlineNode::Math(n) => n.attrs = Some(attrs),
        InlineNode::AutoLink(n) => n.attrs = Some(attrs),
        InlineNode::Extension(n) => n.attrs = Some(attrs),
        _ => {}
    }
}

/// Merge an attribute block onto an inline node, accumulating classes (§15)
/// instead of overwriting -- used for chained blocks (`[x]{.a}{.b}`).
pub(in crate::parse) fn merge_attrs_into_inline(
    node: &mut InlineNode,
    attrs: Attrs,
    slots: &mut Option<HashSet<AttrSlot>>,
) {
    match node {
        InlineNode::Emphasis(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Link(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Image(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Span(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Math(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::AutoLink(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Extension(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Code(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        // A trailing standalone block chains onto an inline literal, promoting a
        // bare literal to a `<span>` (`` !`x`{.a}{.b} `` -> class="a b"). Matches
        // carve-js, whose merge attaches to any non-text node.
        InlineNode::LiteralInline(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::Footnote(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::CriticInsert(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        InlineNode::CriticDelete(n) => merge_attrs_indexed(&mut n.attrs, attrs, slots),
        _ => {}
    }
}

/// Whether an inline node can carry an attribute block (so a following `{...}`
/// attaches rather than staying literal). Text/raw nodes cannot.
pub(crate) fn inline_is_attributable(node: &InlineNode) -> bool {
    matches!(
        node,
        InlineNode::Emphasis(_)
            | InlineNode::Link(_)
            | InlineNode::Image(_)
            | InlineNode::Span(_)
            | InlineNode::Math(_)
            | InlineNode::AutoLink(_)
            | InlineNode::Extension(_)
            | InlineNode::Code(_)
            | InlineNode::LiteralInline(_)
            | InlineNode::Footnote(_)
            | InlineNode::CriticInsert(_)
            | InlineNode::CriticDelete(_)
    )
}
