use crate::ast::{BlockNode, InlineNode, RawInline, Span, Text};
use crate::carrier_markers::carrier_payload;
use crate::include_walk::{visit_block_children, visit_inline_children, SubtreeVisitor};

pub(crate) struct InlineCarriers {
    pub(crate) source: String,
    token: String,
    slots: Vec<(String, usize)>,
}

impl InlineCarriers {
    pub(crate) fn plain(source: &str) -> Self {
        Self {
            source: source.to_owned(),
            token: String::new(),
            slots: Vec::new(),
        }
    }

    pub(crate) fn prepare(source: &str) -> Self {
        if !source.contains(crate::carrier_markers::CARRIER_PREFIX) {
            return Self::plain(source);
        }
        use pulldown_cmark::{Event, Parser, Tag, TagEnd};
        let mut ranges = Vec::new();
        let mut excluded = 0;
        let mut excluded_html_block = false;
        for (event, range) in
            Parser::new_ext(source, pulldown_cmark::Options::all()).into_offset_iter()
        {
            match event {
                Event::Start(Tag::HtmlBlock) => {
                    let first = source[range]
                        .trim_start()
                        .split('\n')
                        .next()
                        .unwrap_or_default();
                    excluded_html_block = !first
                        .starts_with(crate::carrier_markers::CARRIER_PREFIX)
                        || !first.contains(crate::carrier_markers::CARRIER_SUFFIX);
                    excluded += usize::from(excluded_html_block);
                }
                Event::End(TagEnd::HtmlBlock) => {
                    excluded -= usize::from(excluded_html_block);
                    excluded_html_block = false;
                }
                Event::Start(Tag::Table(_) | Tag::Image { .. }) => excluded += 1,
                Event::End(TagEnd::Table | TagEnd::Image) => excluded -= 1,
                Event::Html(_) if excluded == 0 => ranges.push((range, false)),
                Event::InlineHtml(_) if excluded == 0 => ranges.push((range, true)),
                _ => {}
            }
        }
        let mut token = "CARVEINLINECARRIER".to_owned();
        while source.contains(&token) {
            token.push('X');
        }
        let pattern = regex::Regex::new(r"<!-- carve: (?:[^\r\n]*?) -->").unwrap();
        let line_starts: Vec<_> = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        let mut slots = Vec::new();
        let mut out = String::new();
        let mut cursor = 0;
        static HOST_PREFIX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let host_prefix = HOST_PREFIX.get_or_init(|| {
            regex::Regex::new(
                r"^(?:[ \t]|>[ \t]?|(?:[-+*]|[0-9]{1,9}[.)])[ \t]+(?:\[[ xX]\][ \t]+)?)*$",
            )
            .unwrap()
        });
        for found in pattern.find_iter(source) {
            let at = ranges.partition_point(|range| range.0.start <= found.start());
            if at == 0 || ranges[at - 1].0.end < found.end() {
                continue;
            }
            let (range, inline) = &ranges[at - 1];
            if *inline && range.start != found.start() {
                continue;
            }
            let mut comment_cursor = range.start;
            let mut nested = false;
            while let Some(open) = source[comment_cursor..found.start()].find("<!--") {
                let open = comment_cursor + open;
                let Some(close) = source[open + 4..].find("-->") else {
                    nested = true;
                    break;
                };
                comment_cursor = open + 4 + close + 3;
                if comment_cursor > found.start() {
                    nested = true;
                    break;
                }
            }
            if nested {
                continue;
            }
            let Some(payload) = carrier_payload(found.as_str()) else {
                continue;
            };
            if payload.starts_with(':') || payload.starts_with('^') || payload.is_empty() {
                continue;
            }
            let line_start = source[..found.start()].rfind('\n').map_or(0, |at| at + 1);
            let line_end = source[found.end()..]
                .find('\n')
                .map_or(source.len(), |at| found.end() + at);
            let isolated = host_prefix.is_match(&source[line_start..found.start()])
                && source[found.end()..line_end].trim().is_empty();
            if isolated
                && !(payload == "[" || payload.starts_with("]{") || payload.starts_with("</#"))
            {
                continue;
            }
            out.push_str(&source[cursor..found.start()]);
            out.push_str(&format!("{token}{}Z", slots.len()));
            slots.push((
                found.as_str().to_owned(),
                line_starts.partition_point(|at| *at <= found.start()),
            ));
            cursor = found.end();
        }
        out.push_str(&source[cursor..]);
        Self {
            source: out,
            token,
            slots,
        }
    }

    pub(crate) fn restore(&self, source: &str) -> (Option<String>, Vec<(usize, String)>) {
        if self.slots.is_empty() {
            return (Some(source.to_owned()), Vec::new());
        }
        struct Restore<'a> {
            lift: &'a InlineCarriers,
            damaged: bool,
            losses: Vec<(usize, String)>,
        }
        impl Restore<'_> {
            fn attributes(payload: &str) -> Option<Option<crate::ast::Attrs>> {
                if !payload.starts_with('{') || !payload.ends_with('}') {
                    return None;
                }
                let document = crate::parse::parse(&format!("[x]{payload}"));
                match document.children.first() {
                    Some(BlockNode::Paragraph(paragraph)) if paragraph.children.len() == 1 => {
                        match &paragraph.children[0] {
                            InlineNode::Span(span) => Some(span.attrs.clone()),
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            fn split(&self, nodes: Vec<InlineNode>) -> Vec<InlineNode> {
                let mut out = Vec::new();
                for node in nodes {
                    let InlineNode::Text(text) = &node else {
                        out.push(node);
                        continue;
                    };
                    let mut cursor = 0;
                    while let Some(offset) = text.value[cursor..].find(&self.lift.token) {
                        let start = cursor + offset;
                        let digits = start + self.lift.token.len();
                        let end = text.value[digits..].find('Z').map(|at| digits + at);
                        let slot = end
                            .and_then(|end| text.value[digits..end].parse::<usize>().ok())
                            .and_then(|index| self.lift.slots.get(index));
                        let (Some(end), Some((marker, line))) = (end, slot) else {
                            break;
                        };
                        if start > cursor {
                            out.push(InlineNode::Text(Text {
                                value: text.value[cursor..start].to_owned(),
                                pos: text.pos.clone(),
                            }));
                        }
                        out.push(InlineNode::RawInline(RawInline {
                            cell_wraps: None,
                            format: "html".to_owned(),
                            content: marker.clone(),
                            injected: false,
                            pos: Some(crate::ast::Pos {
                                start_line: *line,
                                end_line: *line,
                                ..Default::default()
                            }),
                        }));
                        cursor = end + 1;
                    }
                    if cursor == 0 {
                        out.push(node);
                    } else if cursor < text.value.len() {
                        out.push(InlineNode::Text(Text {
                            value: text.value[cursor..].to_owned(),
                            pos: text.pos.clone(),
                        }));
                    }
                }
                out
            }
        }
        impl SubtreeVisitor for Restore<'_> {
            fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
                for block in blocks {
                    if let BlockNode::Heading(heading) = block {
                        if let Some(attrs) = &mut heading.attrs {
                            if attrs
                                .id
                                .as_ref()
                                .is_some_and(|id| id.contains(&self.lift.token))
                            {
                                attrs.id = None;
                            }
                        }
                    }
                    if !matches!(
                        block,
                        BlockNode::Table(_) | BlockNode::CodeBlock(_) | BlockNode::RawBlock(_)
                    ) {
                        visit_block_children(block, self);
                    }
                }
            }
            fn inlines(&mut self, nodes: &mut Vec<InlineNode>) {
                let expanded = self.split(std::mem::take(nodes));
                let mut stack: Vec<Vec<InlineNode>> = Vec::new();
                let mut current = Vec::new();
                let mut iter = expanded.into_iter().peekable();
                while let Some(mut node) = iter.next() {
                    if let InlineNode::RawInline(raw) = &node {
                        if let Some(payload) = carrier_payload(&raw.content) {
                            if payload == "[" {
                                stack.push(current);
                                current = Vec::new();
                                continue;
                            }
                            if let Some(closer) = payload.strip_prefix(']') {
                                if let Some(attrs) = Self::attributes(closer) {
                                    let Some(mut parent) = stack.pop() else {
                                        self.damaged = true;
                                        current.push(node);
                                        continue;
                                    };
                                    if current.len() >= 2 {
                                        if let (
                                            InlineNode::RawInline(open),
                                            InlineNode::RawInline(close),
                                        ) = (&current[0], &current[current.len() - 1])
                                        {
                                            if open.format == "html"
                                                && open.content.starts_with("<abbr ")
                                                && close.format == "html"
                                                && close.content == "</abbr>"
                                            {
                                                if let Ok(imported) = crate::html_import::html_to_ast(&format!("{}x</abbr>", open.content), &crate::html_import::HtmlImportOptions::default()) {
                                                    if matches!(imported.value.children.first(), Some(BlockNode::Paragraph(paragraph)) if matches!(paragraph.children.first(), Some(InlineNode::Span(span)) if span.attrs.as_ref().and_then(|old| old.key_values.get("abbr")) == attrs.as_ref().and_then(|new| new.key_values.get("abbr")))) {
                                                        current.pop(); current.remove(0);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    let merged = current.len() == 1
                                        && matches!(&current[0], InlineNode::Span(span) if span.attrs == attrs || span.attrs.as_ref().is_some_and(|old| old.id.is_none() && old.classes.is_empty() && old.key_values.len() == 1 && old.key_values.get("abbr") == attrs.as_ref().and_then(|new| new.key_values.get("abbr"))));
                                    if merged {
                                        if let InlineNode::Span(span) = &mut current[0] {
                                            span.attrs = attrs;
                                        }
                                        parent.append(&mut current);
                                    } else {
                                        parent.push(InlineNode::Span(Span {
                                            children: current,
                                            attrs,
                                            injected: false,
                                            pos: None,
                                        }));
                                    }
                                    current = parent;
                                    continue;
                                }
                            } else if let Some(attrs) = Self::attributes(&payload) {
                                match current.last_mut() {
                                    Some(InlineNode::Link(link)) => {
                                        link.attrs = attrs;
                                        link.ref_label = None;
                                        link.raw_ref = None;
                                        continue;
                                    }
                                    Some(InlineNode::Image(image)) => {
                                        image.attrs = attrs;
                                        image.ref_label = None;
                                        image.raw_ref = None;
                                        continue;
                                    }
                                    Some(InlineNode::AutoLink(link)) => {
                                        link.attrs = attrs;
                                        continue;
                                    }
                                    _ => {}
                                }
                            } else if payload.starts_with("</#")
                                && payload.ends_with('>')
                                && matches!(
                                    iter.peek(),
                                    Some(InlineNode::Link(_) | InlineNode::AutoLink(_))
                                )
                            {
                                let document = crate::parse::parse(&payload);
                                if let Some(BlockNode::Paragraph(paragraph)) =
                                    document.children.first()
                                {
                                    if paragraph.children.len() == 1
                                        && matches!(paragraph.children[0], InlineNode::CrossRef(_))
                                    {
                                        current.push(paragraph.children[0].clone());
                                        iter.next();
                                        continue;
                                    }
                                }
                            }
                            if !payload.starts_with(':')
                                && !payload.starts_with('^')
                                && !payload.is_empty()
                            {
                                let line = raw.pos.as_ref().map_or(1, |pos| pos.start_line);
                                self.losses.push((
                                    line,
                                    format!(
                                        "Dropped carrier marker with an invalid payload: {}",
                                        raw.content
                                    ),
                                ));
                            }
                        }
                    }
                    visit_inline_children(&mut node, self);
                    current.push(node);
                }
                if !stack.is_empty() {
                    self.damaged = true;
                }
                *nodes = current;
            }
        }
        let mut document = crate::parse::parse(source);
        let mut restore = Restore {
            lift: self,
            damaged: false,
            losses: Vec::new(),
        };
        restore.blocks(&mut document.children);
        for body in document.footnote_defs.values_mut() {
            restore.blocks(body);
        }
        let value = if restore.damaged {
            None
        } else {
            crate::render_carve::render_carve(&document).ok()
        };
        (value, restore.losses)
    }
}
