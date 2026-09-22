//! Shared, allocation-light Markdown syntax recognition.
//!
//! This module intentionally recognizes the syntax Ekphos already supports; it
//! is not a complete CommonMark parser. Consumers keep ownership of rendering,
//! navigation, validation, and styling policy while sharing byte ranges and
//! syntax boundaries.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heading<'a> {
    pub level: usize,
    pub text: &'a str,
}

/// Recognize an ATX heading with one to six `#` markers.
pub fn heading(line: &str) -> Option<Heading<'_>> {
    let level = line.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &line[level..];
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(Heading { level, text: rest.trim_start().trim_end_matches(|ch: char| ch == '#' || ch.is_whitespace()) })
}

/// Return the zero-based line containing the closing frontmatter delimiter.
pub fn frontmatter_end(content: &str) -> Option<usize> {
    frontmatter_end_in_lines(content.lines())
}

pub fn frontmatter_end_in_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<usize> {
    let mut lines = lines.into_iter();
    if lines.next()?.trim() != "---" {
        return None;
    }
    lines.enumerate().find_map(|(index, line)| (line.trim() == "---").then_some(index + 1))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceMarker {
    Backtick,
    Tilde,
}

/// Recognize the fence forms already supported by Ekphos.
pub fn fence_marker(line: &str) -> Option<FenceMarker> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") {
        Some(FenceMarker::Backtick)
    } else if trimmed.starts_with("~~~") {
        Some(FenceMarker::Tilde)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineMath<'a> {
    /// Byte range including the opening and closing delimiters.
    pub range: Range<usize>,
    pub source: &'a str,
    pub display: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMathDelimiter {
    Dollar,
    Bracket,
}

impl DisplayMathDelimiter {
    fn opening(self) -> &'static str {
        match self {
            Self::Dollar => "$$",
            Self::Bracket => r"\[",
        }
    }

    fn closing(self) -> &'static str {
        match self {
            Self::Dollar => "$$",
            Self::Bracket => r"\]",
        }
    }
}

fn byte_is_escaped(source: &str, index: usize) -> bool {
    source.as_bytes()[..index].iter().rev().take_while(|byte| **byte == b'\\').count() % 2 == 1
}

fn find_unescaped(source: &str, from: usize, needle: &str) -> Option<usize> {
    let mut cursor = from;
    while let Some(relative) = source.get(cursor..)?.find(needle) {
        let index = cursor + relative;
        if !byte_is_escaped(source, index) {
            return Some(index);
        }
        cursor = index + 1;
    }
    None
}

/// Parse an inline math expression beginning exactly at `start`.
///
/// Dollar delimiters next to whitespace are rejected to avoid accidental
/// currency matches. Parenthesized and double-dollar delimiters accept and
/// trim surrounding whitespace. Code-span exclusion is handled by
/// [`visit_inline_math`].
pub fn inline_math_at(source: &str, start: usize) -> Option<InlineMath<'_>> {
    let bytes = source.as_bytes();
    if source.get(start..)?.starts_with("$$") && !byte_is_escaped(source, start) {
        let body_start = start + 2;
        let end = find_unescaped(source, body_start, "$$")?;
        let body = source[body_start..end].trim();
        if body.is_empty() || body.contains('\n') {
            return None;
        }
        return Some(InlineMath { range: start..end + 2, source: body, display: true });
    }
    if bytes.get(start) == Some(&b'$') && !byte_is_escaped(source, start) && bytes.get(start + 1) != Some(&b'$') && (start == 0 || bytes[start - 1] != b'$') {
        let body_start = start + 1;
        let first = source.get(body_start..)?.chars().next()?;
        if first.is_whitespace() || first == '$' {
            return None;
        }
        let mut cursor = body_start;
        while let Some(relative_end) = source[cursor..].find('$') {
            let end = cursor + relative_end;
            if byte_is_escaped(source, end) {
                cursor = end + 1;
                continue;
            }
            if bytes.get(end + 1) == Some(&b'$') {
                cursor = end + 2;
                continue;
            }
            let body = &source[body_start..end];
            if body.contains('\n') || body.chars().next_back().is_some_and(char::is_whitespace) {
                cursor = end + 1;
                continue;
            }
            return Some(InlineMath { range: start..end + 1, source: body, display: false });
        }
        return None;
    }

    if source.get(start..)?.starts_with(r"\(") && !byte_is_escaped(source, start) {
        let body_start = start + 2;
        let mut cursor = body_start;
        while let Some(relative_end) = source[cursor..].find(r"\)") {
            let end = cursor + relative_end;
            if byte_is_escaped(source, end) {
                cursor = end + 2;
                continue;
            }
            let body = source[body_start..end].trim();
            if body.is_empty() || body.contains('\n') {
                return None;
            }
            return Some(InlineMath { range: start..end + 2, source: body, display: false });
        }
    }
    None
}

fn closing_single_marker(source: &str, start: usize, marker: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = start;
    while cursor < bytes.len() {
        if bytes[cursor] == marker && bytes.get(cursor + 1) != Some(&marker) {
            return Some(cursor + 1);
        }
        cursor += 1;
    }
    None
}

fn closing_double_marker(source: &str, start: usize, marker: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = start;
    while cursor + 1 < bytes.len() {
        if bytes[cursor] == marker && bytes[cursor + 1] == marker {
            return Some(cursor + 2);
        }
        cursor += 1;
    }
    None
}

/// Return the end of an inline construct which the content renderer consumes
/// before looking for nested math. This keeps equation discovery aligned with
/// the spans which are actually rendered.
fn non_math_inline_span_end(source: &str, start: usize) -> Option<usize> {
    let rest = source.get(start..)?;
    if rest.starts_with('`') {
        return Some(source[start + 1..].find('`').map_or(source.len(), |end| start + end + 2));
    }
    if rest.starts_with("**") {
        return Some(closing_double_marker(source, start + 2, b'*').unwrap_or(source.len()));
    }
    if rest.starts_with('*') {
        return Some(closing_single_marker(source, start + 1, b'*').unwrap_or(source.len()));
    }
    if rest.starts_with("__") {
        return Some(closing_double_marker(source, start + 2, b'_').unwrap_or(source.len()));
    }
    if rest.starts_with('_') {
        return Some(closing_single_marker(source, start + 1, b'_').unwrap_or(source.len()));
    }
    if rest.starts_with("~~") {
        return Some(closing_double_marker(source, start + 2, b'~').unwrap_or(source.len()));
    }
    if rest.starts_with("!![") {
        return markdown_link_at(source, start + 1).map(|link| link.range.end);
    }
    if rest.starts_with("![") {
        return markdown_link_at(source, start).map(|link| link.range.end);
    }
    if rest.starts_with('[') {
        return wiki_link_at(source, start).map(|link| link.range.end).or_else(|| markdown_link_at(source, start).map(|link| link.range.end));
    }
    if rest.starts_with('h') {
        return bare_url_len(source, start).map(|len| start + len);
    }
    None
}

/// Visit inline math on one source line. Code, links, images, bare URLs, and
/// emphasis take precedence when their opener occurs before a math opener;
/// syntax contained by a math expression stays part of that expression.
pub fn visit_inline_math<'a>(source: &'a str, mut visit: impl FnMut(InlineMath<'a>)) {
    let mut cursor = 0;
    while cursor < source.len() {
        let character = source[cursor..].chars().next().expect("cursor remains on a character boundary");
        if matches!(character, '$' | '\\') {
            if let Some(math) = inline_math_at(source, cursor) {
                cursor = math.range.end;
                visit(math);
                continue;
            }
        }
        if let Some(end) = non_math_inline_span_end(source, cursor) {
            cursor = end;
        } else {
            cursor += character.len_utf8();
        }
    }
}

pub fn inline_math(source: &str) -> Vec<InlineMath<'_>> {
    let mut expressions = Vec::new();
    visit_inline_math(source, |expression| expressions.push(expression));
    expressions
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListMarker {
    pub marker: Range<usize>,
    pub content_start: usize,
}

pub fn list_marker(line: &str) -> Option<ListMarker> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let marker_start = line.len() - trimmed.len();
    let marker_len = if trimmed.starts_with(['-', '*', '+']) {
        1
    } else {
        let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
        if !(1..=9).contains(&digits) || !matches!(trimmed.as_bytes().get(digits), Some(b'.' | b')')) {
            return None;
        }
        digits + 1
    };
    let rest = &trimmed[marker_len..];
    let spacing = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    (spacing > 0).then(|| ListMarker { marker: marker_start..marker_start + marker_len, content_start: marker_start + marker_len + spacing })
}

pub fn block_content_start(line: &str) -> usize {
    list_marker(line).map_or_else(|| line.len() - line.trim_start().len(), |marker| marker.content_start)
}

/// Return the expression from a single-line display-math block (`$$...$$` or
/// `\[...\]`).
pub fn display_math_body(line: &str) -> Option<&str> {
    let content = line[block_content_start(line)..].trim_end();
    [DisplayMathDelimiter::Dollar, DisplayMathDelimiter::Bracket].into_iter().find_map(|delimiter| {
        let inner = content.strip_prefix(delimiter.opening())?.strip_suffix(delimiter.closing())?;
        if byte_is_escaped(content, content.len() - 2) || find_unescaped(inner, 0, delimiter.closing()).is_some() {
            return None;
        }
        let body = inner.trim();
        (!body.is_empty()).then_some(body)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayMathOpening {
    pub delimiter: DisplayMathDelimiter,
    pub body_start: usize,
}

pub fn display_math_opening(line: &str) -> Option<DisplayMathOpening> {
    let content_start = block_content_start(line);
    let content = &line[content_start..];
    [DisplayMathDelimiter::Dollar, DisplayMathDelimiter::Bracket].into_iter().find_map(|delimiter| {
        let rest = content.strip_prefix(delimiter.opening())?;
        if find_unescaped(rest, 0, delimiter.closing()).is_some() || (delimiter == DisplayMathDelimiter::Dollar && rest.starts_with('$')) {
            return None;
        }
        Some(DisplayMathOpening { delimiter, body_start: content_start + delimiter.opening().len() })
    })
}

pub fn display_math_closing(line: &str, opening: DisplayMathDelimiter) -> Option<usize> {
    let trimmed = line.trim_end();
    let body_end = trimmed.strip_suffix(opening.closing())?.len();
    (!byte_is_escaped(trimmed, body_end)).then_some(body_end)
}

/// Find the first matching closer after an opening display-math delimiter.
pub fn find_display_math_closing_line<'a>(opening: DisplayMathDelimiter, lines: impl IntoIterator<Item = (usize, &'a str)>) -> Option<usize> {
    lines.into_iter().find_map(|(line, source)| display_math_closing(source, opening).is_some().then_some(line))
}

/// Advance a multi-line display-math delimiter state. The boolean reports
/// whether `line` is the matching opening or closing delimiter. A new block is
/// opened only when `has_matching_closer` confirms that it is complete.
pub fn update_display_math_block(state: &mut Option<DisplayMathDelimiter>, line: &str, has_matching_closer: impl FnOnce(DisplayMathDelimiter) -> bool) -> bool {
    match *state {
        Some(opening) => {
            let closes = display_math_closing(line, opening).is_some();
            if closes {
                *state = None;
            }
            closes
        }
        None => match display_math_opening(line) {
            Some(opening) if has_matching_closer(opening.delimiter) => {
                *state = Some(opening.delimiter);
                true
            }
            _ => false,
        },
    }
}

/// Recognize a standalone display-math delimiter line.
pub fn is_display_math_delimiter(line: &str) -> bool {
    matches!(line.trim(), "$$" | r"\[" | r"\]")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink<'a> {
    pub range: Range<usize>,
    pub raw: &'a str,
    pub target: &'a str,
    pub heading: Option<&'a str>,
    pub alias: Option<&'a str>,
}

impl<'a> WikiLink<'a> {
    pub fn display_text(&self) -> &'a str {
        self.alias.unwrap_or(self.raw)
    }

    pub fn char_range(&self, source: &str) -> Range<usize> {
        let start = source[..self.range.start].chars().count();
        start..start + source[self.range.clone()].chars().count()
    }
}

/// Parse a wiki link beginning exactly at `start`, using byte offsets.
pub fn wiki_link_at(source: &str, start: usize) -> Option<WikiLink<'_>> {
    let rest = source.get(start..)?;
    let body = rest.strip_prefix("[[")?;
    let close = body.find("]]")?;
    let raw = &body[..close];
    if raw.is_empty() || raw.contains(['[', ']']) {
        return None;
    }
    let (destination, alias) = raw.split_once('|').map(|(destination, alias)| (destination, Some(alias))).unwrap_or((raw, None));
    let (target, heading) = destination.split_once('#').map(|(target, heading)| (target, Some(heading))).unwrap_or((destination, None));
    let end = start + 2 + close + 2;
    Some(WikiLink { range: start..end, raw, target, heading, alias })
}

/// Visit valid wiki links on one source line, excluding inline-code and inline
/// math spans.
pub fn visit_wiki_links<'a>(source: &'a str, mut visit: impl FnMut(WikiLink<'a>)) {
    let mut cursor = 0;
    while cursor < source.len() {
        let character = source[cursor..].chars().next().expect("cursor remains on a character boundary");
        if matches!(character, '$' | '\\') {
            if let Some(math) = inline_math_at(source, cursor) {
                cursor = math.range.end;
                continue;
            }
        }
        if source[cursor..].starts_with("[[") {
            if let Some(link) = wiki_link_at(source, cursor) {
                cursor = link.range.end;
                visit(link);
            } else if let Some(close) = source[cursor + 2..].find("]]") {
                cursor += close + 4;
            } else {
                break;
            }
            continue;
        }
        if character == '`' {
            let Some(closing) = source[cursor + 1..].find('`') else {
                break;
            };
            cursor += closing + 2;
            continue;
        }
        cursor += character.len_utf8();
    }
}

/// Find valid wiki links on one source line, excluding code and math spans.
pub fn wiki_links(source: &str) -> Vec<WikiLink<'_>> {
    let mut links = Vec::new();
    visit_wiki_links(source, |link| links.push(link));
    links
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedWikiLink<'a> {
    pub row: usize,
    pub source: &'a str,
    pub link: WikiLink<'a>,
}

pub fn document_wiki_links_with_tilde_fences(content: &str, skip_through_row: Option<usize>, recognize_tilde_fences: bool) -> Vec<LocatedWikiLink<'_>> {
    let mut links = Vec::new();
    visit_document_wiki_links_with_tilde_fences(content, skip_through_row, recognize_tilde_fences, |link| links.push(link));
    links
}

/// Visit document links without retaining an intermediate collection.
pub fn visit_document_wiki_links_with_tilde_fences<'a>(content: &'a str, skip_through_row: Option<usize>, recognize_tilde_fences: bool, mut visit: impl FnMut(LocatedWikiLink<'a>)) {
    let lines: Vec<&str> = content.lines().collect();
    let mut fence = None;
    let mut math_block = None;
    for (row, line) in lines.iter().copied().enumerate() {
        if skip_through_row.is_some_and(|end| row <= end) {
            continue;
        }
        if math_block.is_some() {
            update_display_math_block(&mut math_block, line, |_| false);
            continue;
        }
        if let Some(marker) = fence_marker(line) {
            if marker == FenceMarker::Tilde && !recognize_tilde_fences {
                visit_wiki_links(line, |link| visit(LocatedWikiLink { row, source: line, link }));
                continue;
            }
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        if update_display_math_block(&mut math_block, line, |opening| find_display_math_closing_line(opening, lines.iter().copied().enumerate().skip(row + 1)).is_some()) {
            continue;
        }
        visit_wiki_links(line, |link| visit(LocatedWikiLink { row, source: line, link }));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownLinkKind {
    Link,
    Image,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownLink<'a> {
    pub range: Range<usize>,
    pub label: &'a str,
    pub destination: &'a str,
    pub kind: MarkdownLinkKind,
}

/// Parse `[label](destination)` or `![alt](destination)` at a byte offset.
pub fn markdown_link_at(source: &str, start: usize) -> Option<MarkdownLink<'_>> {
    let rest = source.get(start..)?;
    let (kind, prefix_len) = if rest.starts_with("![") {
        (MarkdownLinkKind::Image, 2)
    } else if rest.starts_with('[') && !rest.starts_with("[[") {
        (MarkdownLinkKind::Link, 1)
    } else {
        return None;
    };
    let label_end = rest[prefix_len..].find("](")? + prefix_len;
    let destination_start = label_end + 2;
    let destination_end = rest[destination_start..].find(')')? + destination_start;
    let destination = &rest[destination_start..destination_end];
    Some(MarkdownLink { range: start..start + destination_end + 1, label: &rest[prefix_len..label_end], destination, kind })
}

/// Return the byte length of a bare HTTP(S) URL at `start`.
pub fn bare_url_len(source: &str, start: usize) -> Option<usize> {
    let rest = source.get(start..)?;
    let scheme_len = if rest.starts_with("https://") {
        8
    } else if rest.starts_with("http://") {
        7
    } else {
        return None;
    };
    let mut end = rest.len();
    for (index, ch) in rest[scheme_len..].char_indices() {
        if ch.is_whitespace() || matches!(ch, ')' | ']' | '>' | '<' | '"' | '\'' | '|') {
            end = scheme_len + index;
            break;
        }
    }
    while end > scheme_len {
        let last = rest[..end].chars().next_back()?;
        if matches!(last, '.' | ',' | ';' | ':' | '!' | '?') {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    (end > scheme_len).then_some(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_match_supported_atx_boundaries() {
        assert_eq!(heading("# Title").unwrap(), Heading { level: 1, text: "Title" });
        assert_eq!(heading("###### Deep ###").unwrap(), Heading { level: 6, text: "Deep" });
        assert!(heading("#hashtag").is_none());
        assert!(heading("####### too deep").is_none());
    }

    #[test]
    fn frontmatter_requires_opening_and_closing_delimiters() {
        assert_eq!(frontmatter_end("---\ntitle: Note\n---\nBody"), Some(2));
        assert_eq!(frontmatter_end("---\ntitle: Note\nBody"), None);
        assert_eq!(frontmatter_end("Body\n---"), None);
    }

    #[test]
    fn wiki_link_parts_and_unicode_columns_are_stable() {
        let source = "前 [[folder/笔记#标题|别名]] 后";
        let link = wiki_links(source).pop().unwrap();
        assert_eq!(link.target, "folder/笔记");
        assert_eq!(link.heading, Some("标题"));
        assert_eq!(link.alias, Some("别名"));
        assert_eq!(link.display_text(), "别名");
        assert_eq!(link.char_range(source).start, 2);
    }

    #[test]
    fn wiki_scanner_rejects_empty_nested_and_inline_code_links() {
        let links = wiki_links("[[]] [[[nested]]] `[[code]]` **[[bold]]** [[real]]");
        assert_eq!(links.iter().map(|link| link.target).collect::<Vec<_>>(), vec!["bold", "real"]);
    }

    #[test]
    fn document_scanner_skips_frontmatter_and_both_fence_styles() {
        let content = "---\n[[meta]]\n---\n[[one]]\n```md\n[[code]]\n```\n~~~\n[[tilde]]\n~~~\n[[two]]";
        let links = document_wiki_links_with_tilde_fences(content, frontmatter_end(content), true);
        assert_eq!(links.iter().map(|item| item.link.target).collect::<Vec<_>>(), vec!["one", "two"]);
        assert_eq!(links.iter().map(|item| item.row).collect::<Vec<_>>(), vec![3, 10]);
    }

    #[test]
    fn inline_math_respects_boundaries_escapes_and_code() {
        let source = r"before $x_1 + \alpha$ and \( \frac{1}{2} \) `code $ignored$ \(ignored\)` \$cash and $y^2$ after";
        let expressions = inline_math(source);
        assert_eq!(expressions.iter().map(|expression| expression.source).collect::<Vec<_>>(), vec![r"x_1 + \alpha", r"\frac{1}{2}", "y^2"]);
        assert!(inline_math("$ spaced $").is_empty());
        assert_eq!(inline_math("$$display$$"), vec![InlineMath { range: 0..11, source: "display", display: true }]);
        assert_eq!(inline_math("see $$ \\sum_i x_i $$ here").iter().map(|expression| (expression.source, expression.display)).collect::<Vec<_>>(), vec![(r"\sum_i x_i", true)]);
        assert!(inline_math("$$x$").is_empty());
        assert!(inline_math("$$unclosed").is_empty());
        assert!(inline_math(r"\\(escaped\)").is_empty());
        assert!(inline_math(r"\(\)").is_empty());
    }

    #[test]
    fn inline_math_does_not_claim_delimiters_owned_by_other_inline_syntax() {
        let source = r"[link \(link\)](dest) ![image \(image\)](image.png) !![preview \(preview\)](dest) https://example.test/\(url\) **\(bold\)** _\(italic\)_ ~~\(strike\)~~ [[note|\(alias\)]] then \(real\)";
        let expressions = inline_math(source);
        assert_eq!(expressions.iter().map(|expression| expression.source).collect::<Vec<_>>(), vec!["real"]);
    }

    #[test]
    fn inline_math_and_wiki_links_respect_whichever_outer_syntax_opens_first() {
        let source = r"\(\text{[[not-a-link]] and [label](url)}\) [[real|\(literal-math\)]]";
        let expressions = inline_math(source);
        assert_eq!(expressions.len(), 1);
        assert_eq!(expressions[0].source, r"\text{[[not-a-link]] and [label](url)}");
        let links = wiki_links(source);
        assert_eq!(links.iter().map(|link| link.target).collect::<Vec<_>>(), vec!["real"]);
    }

    #[test]
    fn document_wiki_links_skip_complete_math_blocks_but_not_unmatched_openers() {
        let content = "$$\n[[dollar-math]]\n$$\n\\[\n[[bracket-math]]\n\\]\n\\[\n[[real]]";
        let links = document_wiki_links_with_tilde_fences(content, None, true);
        assert_eq!(links.iter().map(|item| item.link.target).collect::<Vec<_>>(), vec!["real"]);
        assert_eq!(links[0].row, 7);
    }

    #[test]
    fn display_math_recognizes_fences_and_single_line_bodies() {
        assert!(is_display_math_delimiter("  $$  "));
        assert!(is_display_math_delimiter(r"  \[  "));
        assert!(is_display_math_delimiter(r"  \]  "));
        assert_eq!(display_math_body("$$ \\frac{1}{2} $$"), Some("\\frac{1}{2}"));
        assert_eq!(display_math_body(r"\[ \sum_{i=1}^n i \]"), Some(r"\sum_{i=1}^n i"));
        assert_eq!(display_math_body("$$"), None);
        assert_eq!(display_math_body(r"\[\]"), None);
        assert_eq!(display_math_body("price $$5"), None);
        assert_eq!(display_math_body("$$a$$ and $$b$$"), None);
        assert_eq!(display_math_body(r"$$ \$ $$"), Some(r"\$"));

        let mut block = None;
        assert!(!update_display_math_block(&mut block, r"\[", |_| false));
        assert_eq!(block, None);
        assert!(update_display_math_block(&mut block, r"\[", |_| true));
        assert_eq!(block, Some(DisplayMathDelimiter::Bracket));
        assert!(!update_display_math_block(&mut block, "$$", |_| true));
        assert_eq!(block, Some(DisplayMathDelimiter::Bracket));
        assert!(!update_display_math_block(&mut block, r"a \\]", |_| true));
        assert!(update_display_math_block(&mut block, r"\]", |_| false));
        assert_eq!(block, None);
        assert!(!update_display_math_block(&mut block, r"\]", |_| true));
        assert_eq!(block, None);
    }

    #[test]
    fn display_math_opens_list_items_like_obsidian() {
        assert_eq!(display_math_body("    - $$x=\\frac{-b}{2a}$$"), Some("x=\\frac{-b}{2a}"));
        assert_eq!(display_math_body("1. \\[ y \\]"), Some("y"));
        assert_eq!(display_math_body("- [ ] $$x$$"), None);
        assert_eq!(display_math_opening("    - $$"), Some(DisplayMathOpening { delimiter: DisplayMathDelimiter::Dollar, body_start: 8 }));
        assert_eq!(display_math_opening("$$\\begin{align}"), Some(DisplayMathOpening { delimiter: DisplayMathDelimiter::Dollar, body_start: 2 }));
        assert_eq!(display_math_opening("2) \\["), Some(DisplayMathOpening { delimiter: DisplayMathDelimiter::Bracket, body_start: 5 }));
        assert_eq!(display_math_opening("$$x$$"), None);
        assert_eq!(display_math_opening("$$$"), None);
        assert_eq!(display_math_opening("text $$"), None);
        assert_eq!(display_math_closing("      $$ ", DisplayMathDelimiter::Dollar), Some(6));
        assert_eq!(display_math_closing("\\end{align}$$", DisplayMathDelimiter::Dollar), Some(11));
        assert_eq!(display_math_closing("cost \\$$", DisplayMathDelimiter::Dollar), None);
        assert_eq!(display_math_closing("$$", DisplayMathDelimiter::Bracket), None);

        let lines = ["    - $$", "      \\begin{align}", "      &a^2 + b^2 = c^2 \\\\", "      \\end{align} ", "      $$", "    - $$ ", "      x", "      $$"];
        assert_eq!(find_display_math_closing_line(DisplayMathDelimiter::Dollar, lines.iter().copied().enumerate().skip(1)), Some(4));
        let mut block = None;
        let delimiter_rows: Vec<usize> = lines.iter().enumerate().filter(|(row, line)| update_display_math_block(&mut block, line, |opening| find_display_math_closing_line(opening, lines.iter().copied().enumerate().skip(row + 1)).is_some())).map(|(row, _)| row).collect();
        assert_eq!(delimiter_rows, vec![0, 4, 5, 7]);
    }

    #[test]
    fn list_markers_cover_unordered_and_ordered_items() {
        assert_eq!(list_marker("  - item"), Some(ListMarker { marker: 2..3, content_start: 4 }));
        assert_eq!(list_marker("12. item"), Some(ListMarker { marker: 0..3, content_start: 4 }));
        assert_eq!(list_marker("3) item"), Some(ListMarker { marker: 0..2, content_start: 3 }));
        assert_eq!(list_marker("-item"), None);
        assert_eq!(list_marker("---"), None);
        assert_eq!(list_marker("1.5 is a number"), None);
        assert_eq!(block_content_start("   $$"), 3);
    }

    #[test]
    fn streaming_and_collecting_document_scans_match() {
        let content = "[[one]] and [[two#heading|alias]]\n```\n[[code]]\n```\n[[three]]";
        let collected = document_wiki_links_with_tilde_fences(content, None, true);
        let mut streamed = Vec::new();
        visit_document_wiki_links_with_tilde_fences(content, None, true, |link| {
            streamed.push((link.row, link.link.range, link.link.raw));
        });
        let collected = collected.iter().map(|link| (link.row, link.link.range.clone(), link.link.raw)).collect::<Vec<_>>();
        assert_eq!(streamed, collected);
    }

    #[test]
    fn markdown_links_and_images_preserve_byte_ranges() {
        let source = "[label](https://example.test) ![alt](image.png)";
        let link = markdown_link_at(source, 0).unwrap();
        assert_eq!(link.label, "label");
        assert_eq!(link.destination, "https://example.test");
        assert_eq!(link.kind, MarkdownLinkKind::Link);
        let image_start = source.find("![").unwrap();
        assert_eq!(markdown_link_at(source, image_start).unwrap().kind, MarkdownLinkKind::Image);
        assert!(markdown_link_at("[[wiki]]", 0).is_none());
        assert_eq!(markdown_link_at("[empty]()", 0).unwrap().destination, "");
    }

    #[test]
    fn bare_urls_stop_at_delimiters_and_sentence_punctuation() {
        assert_eq!(bare_url_len("https://example.test.", 0), Some(20));
        assert_eq!(bare_url_len("(https://example.test)", 1), Some(20));
        assert_eq!(bare_url_len("ftp://example.test", 0), None);
    }
}
