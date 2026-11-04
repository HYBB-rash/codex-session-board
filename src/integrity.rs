use anyhow::{Context, Result, bail, ensure};
use lazy_regex::regex;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Character(char),
    Code(String),
    Fence(String, String),
    Indented(String),
    Link(String, String),
    Url(String),
    Tag(String),
    Literal(String),
}

pub fn unescape_html(text: &str) -> String {
    let pattern = regex!(r"&(?:#x[0-9a-fA-F]+|#[0-9]+|[A-Za-z]+);");
    pattern
        .replace_all(text, |captures: &regex::Captures<'_>| {
            let entity = &captures[0];
            match entity {
                "&amp;" => "&".into(),
                "&lt;" => "<".into(),
                "&gt;" => ">".into(),
                "&quot;" => "\"".into(),
                "&apos;" | "&#39;" => "'".into(),
                "&nbsp;" => "\u{a0}".into(),
                _ => {
                    let code = if entity.starts_with("&#x") {
                        u32::from_str_radix(&entity[3..entity.len() - 1], 16).ok()
                    } else if entity.starts_with("&#") {
                        entity[2..entity.len() - 1].parse().ok()
                    } else {
                        None
                    };
                    code.and_then(char::from_u32)
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| entity.to_string())
                }
            }
        })
        .into_owned()
}

fn prose(text: &str) -> Result<String> {
    let characters: Vec<_> = text.chars().collect();
    let mut result = String::new();
    let mut position = 0;
    while position < characters.len() {
        if characters[position] == '\\' && position + 1 < characters.len() {
            let next = characters[position + 1];
            if next == ':'
                || (next == '_'
                    && position > 0
                    && (characters[position - 1].is_alphanumeric()
                        || characters[position - 1] == '_')
                    && characters
                        .get(position + 2)
                        .is_some_and(|value| value.is_alphanumeric() || *value == '_'))
            {
                result.push(next);
                position += 2;
                continue;
            }
        }
        result.push(characters[position]);
        position += 1;
    }
    let array = regex!(r"\\\[([0-9]+(?:,\s*[0-9]+)+)\\\]");
    let mut normalized = String::new();
    let mut cursor = 0;
    for captures in array.captures_iter(&result) {
        let found = captures.get(0).context("Missing escaped array match")?;
        normalized.push_str(&result[cursor..found.start()]);
        if result[found.end()..].starts_with(['[', '(']) {
            normalized.push_str(found.as_str());
        } else {
            normalized.push('[');
            normalized.push_str(&captures[1]);
            normalized.push(']');
        }
        cursor = found.end();
    }
    normalized.push_str(&result[cursor..]);
    Ok(normalized)
}

enum InlineMatch {
    Token(usize, Token),
    LineBreak(usize),
}

fn inline_code(tail: &str) -> Result<InlineMatch> {
    if tail.starts_with("<code>") {
        let end = tail
            .find("</code>")
            .context("Unsupported code markup cannot be verified")?;
        return Ok(InlineMatch::Token(
            end + 7,
            Token::Code(unescape_html(&tail[6..end])),
        ));
    }
    let count = tail.bytes().take_while(|value| *value == b'`').count();
    let delimiter = "`".repeat(count);
    let remainder = &tail[count..];
    let end = remainder
        .find(&delimiter)
        .context("Unmatched code delimiter cannot be verified")?;
    ensure!(
        !remainder[..end].contains('\n'),
        "Multiline inline code cannot be verified"
    );
    Ok(InlineMatch::Token(
        count + end + count,
        Token::Code(remainder[..end].into()),
    ))
}

fn inline_link(tail: &str) -> Result<Option<InlineMatch>> {
    if let Some(label_end) = tail.find("](")
        && let Some(target_end) = tail[label_end + 2..].find(')')
    {
        let label = &tail[1..label_end];
        let target = &tail[label_end + 2..label_end + 2 + target_end];
        ensure!(
            !target.contains(['(', '\n', ' ', '\t']),
            "Unsupported link syntax"
        );
        return Ok(Some(InlineMatch::Token(
            label_end + 3 + target_end,
            Token::Link(label.into(), target.into()),
        )));
    }
    Ok(None)
}

fn inline_tag(tail: &str) -> Result<Option<InlineMatch>> {
    let line_break = regex!(r"^<br\s*/?>$");
    let unsupported_code = regex!(r"^</?(?:code|pre|script|style|textarea)\b");
    let Some(end) = tail.find('>') else {
        return Ok(None);
    };
    let tag = &tail[..=end];
    if line_break.is_match(tag) {
        return Ok(Some(InlineMatch::LineBreak(end + 1)));
    }
    ensure!(
        !unsupported_code.is_match(tag),
        "Unsupported code markup cannot be verified"
    );
    Ok(Some(InlineMatch::Token(end + 1, Token::Tag(tag.into()))))
}

fn match_inline(tail: &str) -> Result<Option<InlineMatch>> {
    if tail.starts_with("\\<") {
        return Ok(tail
            .find('>')
            .map(|end| InlineMatch::Token(end + 1, Token::Literal(tail[..=end].into()))));
    }
    if tail.starts_with("<code>") || tail.starts_with('`') {
        return inline_code(tail).map(Some);
    }
    if tail.starts_with('[') {
        return inline_link(tail);
    }
    if ["https://", "http://", "codex://"]
        .iter()
        .any(|prefix| tail.starts_with(prefix))
    {
        let end = tail
            .find(|character: char| character.is_whitespace() || "<>()".contains(character))
            .unwrap_or(tail.len());
        return Ok(Some(InlineMatch::Token(
            end,
            Token::Url(tail[..end].into()),
        )));
    }
    if tail.starts_with('<') {
        return inline_tag(tail);
    }
    Ok(None)
}

fn flush_prose(plain: &mut String, result: &mut Vec<Token>) -> Result<()> {
    result.extend(prose(plain)?.chars().map(Token::Character));
    plain.clear();
    Ok(())
}

fn inline(text: &str) -> Result<Vec<Token>> {
    let mut result = Vec::new();
    let mut plain = String::new();
    let mut position = 0;
    while position < text.len() {
        let tail = &text[position..];
        match match_inline(tail)? {
            Some(InlineMatch::LineBreak(length)) => {
                plain.push('\n');
                position += length;
            }
            Some(InlineMatch::Token(length, token)) => {
                flush_prose(&mut plain, &mut result)?;
                result.push(token);
                position += length;
            }
            None => {
                let character = tail.chars().next().context("Missing inline character")?;
                plain.push(character);
                position += character.len_utf8();
            }
        }
    }
    ensure!(
        !plain.contains("](") && !plain.contains("]["),
        "Unsupported link syntax"
    );
    flush_prose(&mut plain, &mut result)?;
    Ok(result)
}

fn fenced_line(lines: &[&str], index: usize, result: &mut Vec<Token>) -> Result<Option<usize>> {
    let fence_pattern = regex!(r"^ {0,3}(`{3,}|~{3,})([^\n]*)$");
    let Some(fence) = fence_pattern.captures(lines[index].trim_end_matches('\n')) else {
        return Ok(None);
    };
    let delimiter = &fence[1];
    let character = delimiter
        .chars()
        .next()
        .context("Missing fence delimiter")?;
    let mut end = index + 1;
    while end < lines.len() {
        let closing = lines[end].trim();
        if closing.len() >= delimiter.len() && closing.chars().all(|value| value == character) {
            break;
        }
        end += 1;
    }
    ensure!(end < lines.len(), "Unclosed code fence cannot be verified");
    result.push(Token::Fence(
        fence[2].into(),
        lines[index + 1..end].concat(),
    ));
    if lines[end].ends_with('\n') {
        result.push(Token::Character('\n'));
    }
    Ok(Some(end + 1))
}

fn normalize_separator(lines: &[&str], index: usize) -> String {
    let separator = regex!(r"^\|(?:[ \t]*:?-{3,}:?[ \t]*\|)+[ \t]*$");
    let line = lines[index];
    let stripped = line.trim_end_matches('\n');
    if separator.is_match(stripped)
        && index > 0
        && lines[index - 1].starts_with('|')
        && lines[index - 1].matches('|').count() == stripped.matches('|').count()
        && (index < 2 || !lines[index - 2].starts_with('|'))
    {
        let count = stripped.matches('|').count() - 1;
        format!(
            "|{}{}",
            "---|".repeat(count),
            if line.ends_with('\n') { "\n" } else { "" }
        )
    } else {
        line.to_string()
    }
}

fn validate_line_breaks(line: &str, parsed: &[Token]) -> Result<()> {
    let break_pattern = regex!(r"<br\s*/?>");
    if !break_pattern.is_match(line.trim_end_matches('\n')) {
        return Ok(());
    }
    let block = regex!(r"^(?:[ \t]{4}|\t|[ \t]*(?:[|>#]|[-+*]\s|[0-9]+[.)]\s|`{3}|~{3}))");
    let mut visible: String = parsed
        .iter()
        .map(|token| match token {
            Token::Character(character) => *character,
            _ => '\u{fffc}',
        })
        .collect();
    if line.ends_with('\n') {
        visible.pop();
    }
    if visible.contains('\n') {
        ensure!(
            !visible
                .split('\n')
                .any(|part| part.is_empty() || block.is_match(part)),
            "Line breaks across block boundaries cannot be verified"
        );
    }
    Ok(())
}

fn plain_line(lines: &[&str], index: usize) -> Result<Vec<Token>> {
    let reference_link = regex!(r"^[ \t]*\[[^\]]+\]:");
    let nested_fence = regex!(r"^[ \t]*(?:>[ \t]*|[-+*][ \t]+|[0-9]+[.)][ \t]+)+[`~]{3}");
    let line = lines[index];
    ensure!(
        !nested_fence.is_match(line),
        "Nested code fence cannot be verified"
    );
    ensure!(!reference_link.is_match(line), "Unsupported link syntax");
    let parsed = inline(&normalize_separator(lines, index))?;
    validate_line_breaks(line, &parsed)?;
    Ok(parsed)
}

fn tokens(text: &str) -> Result<Vec<Token>> {
    ensure!(!text.contains('\0'), "NUL content cannot be verified");
    let lines: Vec<_> = text.split_inclusive('\n').collect();
    let mut result = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let stripped = line.trim_end_matches('\n');
        if let Some(next) = fenced_line(&lines, index, &mut result)? {
            index = next;
            continue;
        }
        if line.starts_with("    ")
            || line.starts_with('\t')
            || line.trim_start_matches(' ').starts_with('>')
        {
            result.push(Token::Indented(stripped.into()));
            if line.ends_with('\n') {
                result.push(Token::Character('\n'));
            }
        } else {
            result.extend(plain_line(&lines, index)?);
        }
        index += 1;
    }
    while result.first() == Some(&Token::Character('\n')) {
        result.remove(0);
    }
    while result.last() == Some(&Token::Character('\n')) {
        result.pop();
    }
    Ok(result)
}

pub fn assert_content_equal(expected: &str, actual: &str, context: &str) -> Result<()> {
    let wanted = tokens(expected)?;
    let received = tokens(actual)?;
    let filename = regex!(r"^[A-Za-z0-9][A-Za-z0-9._-]*\.(?:py|zip)(?::[0-9]+)?$");
    let boundary = |token: Option<&Token>| match token {
        Some(Token::Character(character)) => {
            !character.is_ascii_alphanumeric() && !"_./:@?&#%+~=-[]".contains(*character)
        }
        _ => true,
    };
    let mut index = 0;
    for token in received {
        if wanted.get(index) == Some(&token) {
            index += 1;
            continue;
        }
        if let Token::Link(label, target) = token {
            let end = index + label.chars().count();
            let label_tokens: Vec<_> = label.chars().map(Token::Character).collect();
            let mut after = wanted.get(end);
            if after == Some(&Token::Character('.'))
                && wanted.get(end + 1).is_none_or(|value| matches!(value, Token::Character(character) if character.is_whitespace()))
            {
                after = None;
            }
            if filename.is_match(&label)
                && target == format!("http://{label}")
                && boundary(index.checked_sub(1).and_then(|before| wanted.get(before)))
                && boundary(after)
                && wanted.get(index..end) == Some(&label_tokens[..])
            {
                index = end;
                continue;
            }
        }
        bail!("{context}: content differs at token {index}");
    }
    ensure!(
        index == wanted.len(),
        "{context}: content is incomplete at token {index}"
    );
    Ok(())
}

pub fn assert_history_equal(expected: &str, actual: &str) -> Result<()> {
    ensure!(
        expected.trim_matches('\n') == actual.trim_matches('\n'),
        "Pre-delivery history changed"
    );
    Ok(())
}

pub fn assert_delivery_equal(baseline: &str, appended: &str, actual: &str) -> Result<()> {
    let history = baseline.trim_matches('\n');
    let body = actual.trim_matches('\n');
    ensure!(
        body.starts_with(history),
        "Delivery readback history changed or missing"
    );
    let mut suffix = &body[history.len()..];
    if !history.is_empty() {
        let boundary = suffix.len() - suffix.trim_start_matches('\n').len();
        let expected_boundary = baseline.len() - baseline.trim_end_matches('\n').len()
            + appended.len()
            - appended.trim_start_matches('\n').len();
        ensure!(
            boundary > 0 && (boundary == expected_boundary || boundary == 2),
            "Delivery history boundary changed"
        );
        suffix = &suffix[boundary..];
    }
    assert_content_equal(appended, suffix, "Delivery readback verification failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_only_known_rendering_changes() -> Result<()> {
        for (before, after) in [
            ("hello_world: [2, 4]", r"hello\_world\: \[2, 4\]"),
            ("`a<b`", "<code>a&lt;b</code>"),
            ("hello\nworld", "hello<br>world"),
            ("run script.py.", "run [script.py](http://script.py)."),
            (
                "| a | b |\n| :--- | ---: |\n| 1 | 2 |",
                "| a | b |\n|---|---|\n| 1 | 2 |",
            ),
        ] {
            assert_content_equal(before, after, "round trip")?;
        }
        Ok(())
    }

    #[test]
    fn preserves_code_links_tail_and_history() {
        for (before, after) in [
            ("`x_y`", "`x\\_y`"),
            ("[report](https://a.example)", "[report](https://b.example)"),
            (
                "```rust\nlet ready = true;\n```",
                "```rust\nlet ready = false;\n```",
            ),
            ("result\nlast line", "result"),
            ("first\nsecond", "second\nfirst"),
        ] {
            assert!(assert_content_equal(before, after, "readback").is_err());
        }
        assert!(assert_history_equal("saved", "changed").is_err());
        assert!(assert_delivery_equal("history", "\n\nreport\ntail", "history\n\nreport").is_err());
        assert!(assert_content_equal("unclosed `", "unclosed `", "readback").is_err());
    }
}
