use std::borrow::Cow;

const UNNUMBERED_ENVIRONMENTS: [(&str, &str); 10] =
    [("align", "align*"), ("alignat", "alignat*"), ("equation", "equation*"), ("gather", "gather*"), ("multline", "gather*"), ("multline*", "gather*"), ("flalign", "align*"), ("flalign*", "align*"), ("eqnarray", "darray}{rcl"), ("eqnarray*", "darray}{rcl")];

const SHIMS: [(&str, &str); 13] = [
    ("newcommand", r"\def\newcommand{\providecommand}"),
    ("renewcommand", r"\def\renewcommand{\providecommand}"),
    ("DeclareMathOperator", r"\def\DeclareMathOperator#1#2{\providecommand{#1}{\operatorname{#2}}}"),
    ("DeclareMathOperatorStar", r"\def\DeclareMathOperatorStar#1#2{\providecommand{#1}{\operatorname*{#2}}}"),
    ("label", r"\def\label#1{}"),
    ("eqref", r"\def\eqref#1{(\text{???})}"),
    ("ref", r"\def\ref#1{\text{???}}"),
    ("require", r"\def\require#1{}"),
    ("displaylines", r"\def\displaylines#1{\begin{gathered}#1\end{gathered}}"),
    ("intertext", r"\def\intertext#1{\text{#1}\\}"),
    ("class", r"\def\class#1#2{#2}"),
    ("cssId", r"\def\cssId#1#2{#2}"),
    ("style", r"\def\style#1#2{#2}"),
];

fn control_word_end(source: &str, start: usize) -> usize {
    start + source[start..].bytes().take_while(u8::is_ascii_alphabetic).count()
}

fn skip_whitespace(source: &str, start: usize) -> usize {
    start + source[start..].bytes().take_while(u8::is_ascii_whitespace).count()
}

fn group_end(source: &str, start: usize, open: u8, close: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(start) != Some(&open) {
        return None;
    }
    let mut depth = 0usize;
    let mut cursor = start;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => cursor += 1,
            byte if byte == open => depth += 1,
            byte if byte == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(cursor + 1);
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn control_sequence_end(source: &str, start: usize) -> Option<usize> {
    if source.as_bytes().get(start) != Some(&b'\\') {
        return None;
    }
    let end = control_word_end(source, start + 1);
    if end > start + 1 {
        return Some(end);
    }
    source[start + 1..].chars().next().map(|character| start + 1 + character.len_utf8())
}

fn defined_name_end(source: &str, start: usize) -> Option<usize> {
    let start = skip_whitespace(source, start);
    group_end(source, start, b'{', b'}').or_else(|| control_sequence_end(source, start))
}

fn command_definition_end(source: &str, start: usize) -> Option<usize> {
    let mut cursor = skip_whitespace(source, defined_name_end(source, start)?);
    for _ in 0..2 {
        if let Some(end) = group_end(source, cursor, b'[', b']') {
            cursor = skip_whitespace(source, end);
        }
    }
    group_end(source, cursor, b'{', b'}')
}

fn def_definition_end(source: &str, start: usize) -> Option<usize> {
    let name_end = control_sequence_end(source, skip_whitespace(source, start))?;
    let body_start = name_end + source[name_end..].find('{')?;
    if source[name_end..body_start].contains(['}', '\\']) {
        return None;
    }
    group_end(source, body_start, b'{', b'}')
}

fn operator_definition_end(source: &str, start: usize) -> Option<usize> {
    let start = if source[start..].starts_with('*') { start + 1 } else { start };
    let name_end = defined_name_end(source, start)?;
    group_end(source, skip_whitespace(source, name_end), b'{', b'}')
}

fn visit_control_words(source: &str, mut visit: impl FnMut(usize, &str) -> Option<usize>) {
    let mut cursor = 0;
    while let Some(relative) = source[cursor..].find('\\') {
        let start = cursor + relative;
        let name_end = control_word_end(source, start + 1);
        if name_end == start + 1 {
            cursor = start + 1 + source[start + 1..].chars().next().map_or(0, char::len_utf8);
            continue;
        }
        cursor = visit(start, &source[start + 1..name_end]).unwrap_or(name_end);
    }
}

fn contains_command(source: &str, command: &str) -> bool {
    let mut found = false;
    visit_control_words(source, |_, name| {
        found |= name == command;
        found.then_some(source.len())
    });
    found
}

pub fn macro_definitions(source: &str) -> Vec<&str> {
    let mut definitions = Vec::new();
    visit_control_words(source, |start, name| {
        let name_end = start + 1 + name.len();
        let end = match name {
            "newcommand" | "renewcommand" | "providecommand" => command_definition_end(source, name_end),
            "def" | "gdef" => def_definition_end(source, name_end),
            "DeclareMathOperator" => operator_definition_end(source, name_end),
            _ => None,
        }?;
        definitions.push(&source[start..end]);
        Some(end)
    });
    definitions
}

fn strip_bbox_options(source: &str) -> Cow<'_, str> {
    if !contains_command(source, "bbox") {
        return Cow::Borrowed(source);
    }
    let mut result = String::with_capacity(source.len());
    let mut copied = 0;
    visit_control_words(source, |start, name| {
        if name != "bbox" {
            return None;
        }
        let options_start = skip_whitespace(source, start + 5);
        let end = group_end(source, options_start, b'[', b']').unwrap_or(start + 5);
        result.push_str(&source[copied..start]);
        copied = end;
        Some(end)
    });
    result.push_str(&source[copied..]);
    Cow::Owned(result)
}

pub fn mathjax_compatible(source: &str) -> String {
    let mut source = strip_bbox_options(source).replace(r"\DeclareMathOperator*", r"\DeclareMathOperatorStar");
    for (environment, replacement) in UNNUMBERED_ENVIRONMENTS {
        for keyword in ["begin", "end"] {
            let from = format!(r"\{keyword}{{{environment}}}");
            if source.contains(&from) {
                let replacement = if keyword == "end" { replacement.split('}').next().unwrap_or(replacement) } else { replacement };
                source = source.replace(&from, &format!(r"\{keyword}{{{replacement}}}"));
            }
        }
    }
    let shims: String = SHIMS.iter().filter(|(command, _)| contains_command(&source, command)).map(|(_, definition)| *definition).collect();
    if shims.is_empty() {
        source
    } else {
        shims + &source
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_environments_render_unnumbered_like_obsidian() {
        assert_eq!(mathjax_compatible(r"\begin{align}a&=b\end{align}"), r"\begin{align*}a&=b\end{align*}");
        assert_eq!(mathjax_compatible(r"\begin{alignat}{2}a&=b\end{alignat}"), r"\begin{alignat*}{2}a&=b\end{alignat*}");
        assert_eq!(mathjax_compatible(r"\begin{eqnarray}a&=&b\end{eqnarray}"), r"\begin{darray}{rcl}a&=&b\end{darray}");
        assert_eq!(mathjax_compatible(r"\begin{multline}a\\b\end{multline}"), r"\begin{gather*}a\\b\end{gather*}");
        assert_eq!(mathjax_compatible(r"\begin{aligned}a&=b\end{aligned}"), r"\begin{aligned}a&=b\end{aligned}");
        assert_eq!(mathjax_compatible(r"\begin{align*}a\end{align*}"), r"\begin{align*}a\end{align*}");
    }

    #[test]
    fn shims_are_added_only_for_commands_in_use() {
        assert_eq!(mathjax_compatible(r"x^2"), "x^2");
        assert_eq!(mathjax_compatible(r"\eqref{a}"), r"\def\eqref#1{(\text{???})}\eqref{a}");
        assert_eq!(mathjax_compatible(r"\labelled"), r"\labelled");
        assert_eq!(mathjax_compatible(r"\\label"), r"\\label");
        assert_eq!(mathjax_compatible(r"\bbox[5px, border: 1px solid red]{x} + \bbox{y}"), r"{x} + {y}");
        assert_eq!(mathjax_compatible(r"\DeclareMathOperator*{\argmax}{arg\,max}"), r"\def\DeclareMathOperatorStar#1#2{\providecommand{#1}{\operatorname*{#2}}}\DeclareMathOperatorStar{\argmax}{arg\,max}");
    }

    #[test]
    fn macro_definitions_capture_complete_definitions_only() {
        let source = r"\newcommand{\R}{\mathbb{R}} \renewcommand\vec[1]{\mathbf{#1}} \def\pair#1#2{(#1, #2)} \DeclareMathOperator*{\argmax}{arg\,max} \newcommand{\broken} \R";
        assert_eq!(macro_definitions(source), vec![r"\newcommand{\R}{\mathbb{R}}", r"\renewcommand\vec[1]{\mathbf{#1}}", r"\def\pair#1#2{(#1, #2)}", r"\DeclareMathOperator*{\argmax}{arg\,max}"]);
        assert_eq!(macro_definitions(r"\newcommand{\set}[2][x]{\{#1 \mid #2\}}"), vec![r"\newcommand{\set}[2][x]{\{#1 \mid #2\}}"]);
        assert!(macro_definitions(r"\text{é}\\def").is_empty());
    }
}
