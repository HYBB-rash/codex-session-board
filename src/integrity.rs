use anyhow::{Result, bail, ensure};
use regex::Regex;

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
    let pattern = Regex::new(r"&(?:#x[0-9a-fA-F]+|#[0-9]+|[A-Za-z]+);").unwrap();
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

fn prose(text: &str) -> String {
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
    let array = Regex::new(r"\\\[([0-9]+(?:,\s*[0-9]+)+)\\\]").unwrap();
    let mut normalized = String::new();
    let mut cursor = 0;
    for captures in array.captures_iter(&result) {
        let found = captures.get(0).unwrap();
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
    normalized
}

fn inline(text: &str) -> Result<Vec<Token>> {
    let line_break = Regex::new(r"^<br\s*/?>$")?;
    let unsupported_code = Regex::new(r"^</?(?:code|pre|script|style|textarea)\b")?;
    let mut result = Vec::new();
    let mut plain = String::new();
    let mut position = 0;
    let flush = |plain: &mut String, result: &mut Vec<Token>| {
        result.extend(prose(plain).chars().map(Token::Character));
        plain.clear();
    };
    while position < text.len() {
        let tail = &text[position..];
        let mut matched: Option<(usize, Token)> = None;
        if tail.starts_with("\\<") {
            if let Some(end) = tail.find('>') {
                matched = Some((end + 1, Token::Literal(tail[..=end].into())));
            }
        } else if tail.starts_with("<code>") {
            if let Some(end) = tail.find("</code>") {
                matched = Some((end + 7, Token::Code(unescape_html(&tail[6..end]))));
            } else {
                bail!("Unsupported code markup cannot be verified");
            }
        } else if tail.starts_with('`') {
            let count = tail.bytes().take_while(|value| *value == b'`').count();
            let delimiter = "`".repeat(count);
            let remainder = &tail[count..];
            let end = remainder
                .find(&delimiter)
                .ok_or_else(|| anyhow::anyhow!("Unmatched code delimiter cannot be verified"))?;
            ensure!(
                !remainder[..end].contains('\n'),
                "Multiline inline code cannot be verified"
            );
            matched = Some((count + end + count, Token::Code(remainder[..end].into())));
        } else if tail.starts_with('[') {
            if let Some(label_end) = tail.find("](")
                && let Some(target_end) = tail[label_end + 2..].find(')')
            {
                let label = &tail[1..label_end];
                let target = &tail[label_end + 2..label_end + 2 + target_end];
                ensure!(
                    !target.contains(['(', '\n', ' ', '\t']),
                    "Unsupported link syntax"
                );
                matched = Some((
                    label_end + 3 + target_end,
                    Token::Link(label.into(), target.into()),
                ));
            }
        } else if ["https://", "http://", "codex://"]
            .iter()
            .any(|prefix| tail.starts_with(prefix))
        {
            let end = tail
                .find(|character: char| character.is_whitespace() || "<>()".contains(character))
                .unwrap_or(tail.len());
            matched = Some((end, Token::Url(tail[..end].into())));
        } else if tail.starts_with('<')
            && let Some(end) = tail.find('>')
        {
            let tag = &tail[..=end];
            if line_break.is_match(tag) {
                plain.push('\n');
                position += end + 1;
                continue;
            }
            ensure!(
                !unsupported_code.is_match(tag),
                "Unsupported code markup cannot be verified"
            );
            matched = Some((end + 1, Token::Tag(tag.into())));
        }
        if let Some((length, token)) = matched {
            flush(&mut plain, &mut result);
            result.push(token);
            position += length;
        } else {
            let character = tail.chars().next().unwrap();
            plain.push(character);
            position += character.len_utf8();
        }
    }
    ensure!(
        !plain.contains("](") && !plain.contains("]["),
        "Unsupported link syntax"
    );
    flush(&mut plain, &mut result);
    Ok(result)
}

fn tokens(text: &str) -> Result<Vec<Token>> {
    let reference_link = Regex::new(r"^[ \t]*\[[^\]]+\]:")?;
    ensure!(!text.contains('\0'), "NUL content cannot be verified");
    let fence_pattern = Regex::new(r"^ {0,3}(`{3,}|~{3,})([^\n]*)$")?;
    let nested_fence = Regex::new(r"^[ \t]*(?:>[ \t]*|[-+*][ \t]+|[0-9]+[.)][ \t]+)+[`~]{3}")?;
    let block = Regex::new(r"^(?:[ \t]{4}|\t|[ \t]*(?:[|>#]|[-+*]\s|[0-9]+[.)]\s|`{3}|~{3}))")?;
    let separator = Regex::new(r"^\|(?:[ \t]*:?-{3,}:?[ \t]*\|)+[ \t]*$")?;
    let break_pattern = Regex::new(r"<br\s*/?>")?;
    let lines: Vec<_> = text.split_inclusive('\n').collect();
    let mut result = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let stripped = line.trim_end_matches('\n');
        if let Some(fence) = fence_pattern.captures(stripped) {
            let delimiter = &fence[1];
            let character = delimiter.chars().next().unwrap();
            let mut end = index + 1;
            while end < lines.len() {
                let closing = lines[end].trim();
                if closing.len() >= delimiter.len()
                    && closing.chars().all(|value| value == character)
                {
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
            index = end + 1;
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
            ensure!(
                !nested_fence.is_match(line),
                "Nested code fence cannot be verified"
            );
            ensure!(!reference_link.is_match(line), "Unsupported link syntax");
            let normalized = if separator.is_match(stripped)
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
            };
            let parsed = inline(&normalized)?;
            if break_pattern.is_match(stripped) {
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
            }
            result.extend(parsed);
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
    let filename = Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]*\.(?:py|zip)(?::[0-9]+)?$")?;
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
