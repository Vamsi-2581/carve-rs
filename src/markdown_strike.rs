use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

use pulldown_cmark::{BrokenLink, Event, LinkType, Options, Parser, Tag, TagEnd};
use regex::Regex;

const COMMENT: &str = "<!---->";

pub(crate) struct PreparedStrikes<'a> {
    pub text: Cow<'a, str>,
    comments: Vec<usize>,
    references: HashMap<(usize, usize), (String, String, bool)>,
}

impl PreparedStrikes<'_> {
    pub fn synthetic_comment(&self, event: &Event<'_>, range: &Range<usize>) -> bool {
        matches!(event, Event::InlineHtml(value) if value.as_ref() == COMMENT)
            && self.comments.binary_search(&range.start).is_ok()
    }

    pub fn original_range(&self, range: Range<usize>) -> Range<usize> {
        let original = |at| {
            at - COMMENT.len()
                * self
                    .comments
                    .partition_point(|&start| start + COMMENT.len() <= at)
        };
        original(range.start)..original(range.end)
    }

    pub fn reference_has_title(&self, range: &Range<usize>) -> Option<bool> {
        self.references
            .get(&(range.start, range.end))
            .map(|(_, _, title)| *title)
    }

    pub fn reference(
        &self,
        link: BrokenLink<'_>,
    ) -> Option<(
        pulldown_cmark::CowStr<'static>,
        pulldown_cmark::CowStr<'static>,
    )> {
        let range = self.original_range(link.span);
        self.references
            .get(&(range.start, range.end))
            .map(|(destination, title, _)| (destination.clone().into(), title.clone().into()))
    }
}

fn punctuation(ch: char) -> bool {
    static PUNCTUATION: OnceLock<Regex> = OnceLock::new();
    let mut buffer = [0; 4];
    ch.is_ascii_punctuation()
        || PUNCTUATION
            .get_or_init(|| Regex::new(r"^\p{P}$").unwrap())
            .is_match(ch.encode_utf8(&mut buffer))
}

fn word_neighbor(value: Option<char>) -> bool {
    value.is_some_and(|ch| !ch.is_whitespace() && !punctuation(ch))
}

/// Return the literal delimiter when pulldown pairs underscores whose
/// flanks cmark-gfm rejects. Keep interior events, source coordinates, and
/// reference labels intact.
pub(crate) fn literal_underscore(
    source: &str,
    event: &Event<'_>,
    range: &Range<usize>,
) -> Option<Range<usize>> {
    let (count, opening) = match event {
        Event::Start(Tag::Emphasis) => (1, true),
        Event::End(TagEnd::Emphasis) => (1, false),
        Event::Start(Tag::Strong) => (2, true),
        Event::End(TagEnd::Strong) => (2, false),
        _ => return None,
    };
    let bytes = source.as_bytes();
    let start = range.start;
    let end = range.end;
    if !bytes[start..start + count].iter().all(|&byte| byte == b'_')
        || !bytes[end - count..end].iter().all(|&byte| byte == b'_')
    {
        return None;
    }
    let flanks = |at: usize| {
        let mut start = at;
        let mut end = at + count;
        while start > 0 && bytes[start - 1] == b'_' {
            start -= 1;
        }
        while bytes.get(end) == Some(&b'_') {
            end += 1;
        }
        if bytes.get(start.wrapping_sub(1)) != Some(&b'~') && bytes.get(end) != Some(&b'~') {
            return (true, true);
        }
        // cmark-gfm skips registered extension delimiters during this scan.
        let before = source[..start].chars().rev().find(|&ch| ch != '~');
        let after = source[end..].chars().find(|&ch| ch != '~');
        let before_space = before.map_or(true, char::is_whitespace);
        let after_space = after.map_or(true, char::is_whitespace);
        let before_punct = before.is_some_and(punctuation);
        let after_punct = after.is_some_and(punctuation);
        let left = !after_space && (!after_punct || before_space || before_punct);
        let right = !before_space && (!before_punct || after_space || after_punct);
        (
            left && (!right || before_punct),
            right && (!left || after_punct),
        )
    };
    if flanks(start).0 && flanks(end - count).1 {
        return None;
    }
    Some(if opening {
        start..start + count
    } else {
        end - count..end
    })
}

/// Supply intraword flanking to the Markdown reader without changing tilde
/// run lengths. Synthetic comments are removed from its events; offsets and
/// resolved reference targets retain their original coordinates.
pub(crate) fn prepare(source: &str, options: Options) -> PreparedStrikes<'_> {
    let mut result = PreparedStrikes {
        text: Cow::Borrowed(source),
        comments: Vec::new(),
        references: HashMap::new(),
    };
    let bytes = source.as_bytes();
    if !bytes.contains(&b'~') {
        return result;
    }
    let mut candidates = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'~' {
            at += 1;
            continue;
        }
        let end = at + bytes[at..].iter().take_while(|&&byte| byte == b'~').count();
        if end == at + 1
            && word_neighbor(source[..at].chars().next_back())
            && word_neighbor(source[end..].chars().next())
        {
            candidates.push(at);
        }
        at = end;
    }
    if candidates.is_empty() {
        return result;
    }
    let mut eligible = vec![false; source.len()];
    let mut owners: Vec<Range<usize>> = Vec::new();
    let mut opaque = 0usize;
    let mut autolink = false;
    let mut parser = Parser::new_ext(source, options).into_offset_iter();
    while let Some((event, range)) = parser.next() {
        match &event {
            Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::MetadataBlock(_)) => opaque += 1,
            Event::End(TagEnd::CodeBlock | TagEnd::HtmlBlock | TagEnd::MetadataBlock(_)) => {
                opaque = opaque.saturating_sub(1);
            }
            Event::End(TagEnd::Link) => autolink = false,
            _ => {}
        }
        match &event {
            Event::Start(
                Tag::Link {
                    link_type,
                    dest_url,
                    title,
                    id,
                    ..
                }
                | Tag::Image {
                    link_type,
                    dest_url,
                    title,
                    id,
                    ..
                },
            ) => {
                if let Some(previous) = owners
                    .last_mut()
                    .filter(|previous| previous.end >= range.start)
                {
                    previous.end = previous.end.max(range.end);
                } else {
                    owners.push(range.clone());
                }
                autolink = matches!(link_type, LinkType::Autolink | LinkType::Email);
                if matches!(
                    link_type,
                    LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
                ) {
                    let target = (
                        dest_url.to_string(),
                        title.to_string(),
                        parser
                            .reference_definitions()
                            .get(id)
                            .is_some_and(|definition| definition.title.is_some()),
                    );
                    result
                        .references
                        .insert((range.start, range.end), target.clone());
                    if bytes.get(range.start) == Some(&b'!') {
                        result
                            .references
                            .insert((range.start + 1, range.end), target);
                    }
                }
            }
            Event::Text(_) if opaque == 0 && !autolink => {
                eligible[range.clone()].fill(true);
            }
            Event::Start(Tag::Strikethrough) if opaque == 0 && !autolink => {
                let mut start = range.start;
                while start < range.end && bytes[start] == b'~' {
                    eligible[start] = true;
                    start += 1;
                }
                let mut end = range.end;
                while end > range.start && bytes[end - 1] == b'~' {
                    end -= 1;
                    eligible[end] = true;
                }
            }
            _ => {}
        }
    }
    for email in crate::markdown_gfm_autolinks::bare_links(
        source,
        crate::markdown_gfm_autolinks::Mode::Email,
    ) {
        let owner = owners.partition_point(|owner| owner.start <= email.start);
        if owner == 0 || owners[owner - 1].end <= email.start {
            eligible[email.start..email.end].fill(false);
        }
    }
    let mut insertions = Vec::new();
    for at in candidates {
        if eligible[at] {
            insertions.extend([at, at + 1]);
        }
    }
    if insertions.is_empty() {
        return result;
    }
    let mut text = String::with_capacity(source.len() + insertions.len() * COMMENT.len());
    let mut from = 0;
    for at in insertions {
        text.push_str(&source[from..at]);
        result.comments.push(text.len());
        text.push_str(COMMENT);
        from = at;
    }
    text.push_str(&source[from..]);
    result.text = Cow::Owned(text);
    result
}
