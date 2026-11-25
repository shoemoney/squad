//! Structural scanner. Every token span refers to the original UTF-8 input.
use super::{ParseError, Span, errors::Rule};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Text,
    Protected,
    Escape,
    Mention(String),
    Exclusion(String),
    Command(String),
    Variable(String),
    Assignment { name: String, value: String },
    Separator,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn tokenize(raw: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut exclusions = false;
    while i < raw.len() {
        let start = i;
        let ch = raw[i..].chars().next().unwrap();
        let boundary =
            i == 0 || raw[..i].ends_with(|c: char| c.is_whitespace() || "([{".contains(c));
        let line_start = raw[..i].rfind('\n').map_or(0, |p| p + 1);
        let prefix = &raw[line_start..i];
        let line_end = raw[i..].find('\n').map_or(raw.len(), |p| i + p);
        let mut kind = TokenKind::Text;
        let mut end = i + ch.len_utf8();
        if i == line_start && raw[i..line_end].trim_matches([' ', '\t', '\r']) == "--" {
            kind = TokenKind::Separator;
            end = line_end;
        } else if ch == '\\' && raw[end..].starts_with(['@', '$', '\\', '`', '/']) {
            kind = TokenKind::Escape;
            end += 1;
        } else if ch == '`' {
            kind = TokenKind::Protected;
            if raw[i..].starts_with("```") && prefix.len() <= 3 && prefix.bytes().all(|b| b == b' ')
            {
                let mut next = if line_end < raw.len() {
                    line_end + 1
                } else {
                    raw.len()
                };
                let mut close = None;
                while next < raw.len() {
                    let e = raw[next..].find('\n').map_or(raw.len(), |p| next + p);
                    if raw[next..e].trim() == "```" {
                        close = Some(e);
                        break;
                    }
                    next = if e < raw.len() { e + 1 } else { e };
                }
                end = close.ok_or_else(|| {
                    Rule::Fence.error(
                        Span {
                            start,
                            end: raw.len(),
                        },
                        &[],
                    )
                })?;
            } else {
                let rest = &raw[end..line_end];
                end += rest.find('`').ok_or_else(|| {
                    Rule::Inline.error(
                        Span {
                            start,
                            end: line_end,
                        },
                        &[],
                    )
                })? + 1;
            }
        } else if boundary
            && (ch == '@' || ch == '$')
            && raw[end..].starts_with(|c: char| c.is_ascii_alphabetic())
        {
            while end < raw.len() && raw.as_bytes()[end].is_ascii_alphanumeric()
                || end < raw.len() && matches!(raw.as_bytes()[end], b'_' | b'-')
            {
                end += 1;
            }
            let name = &raw[i + 1..end];
            if ch == '@' {
                kind = TokenKind::Mention(name.to_owned());
            } else {
                let upper = name
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
                let lower = name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
                if !upper && !lower {
                    return Err(Rule::InvalidToken
                        .error(Span { start, end }, &[("token", &raw[start..end])]));
                }
                if upper && raw[end..].starts_with('=') {
                    end += 1;
                    let value_start = end;
                    while end < raw.len() {
                        let c = raw[end..].chars().next().unwrap();
                        if c.is_whitespace() {
                            break;
                        }
                        end += c.len_utf8();
                    }
                    kind = TokenKind::Assignment {
                        name: name.to_owned(),
                        value: raw[value_start..end].to_owned(),
                    };
                } else if upper {
                    kind = TokenKind::Variable(name.to_owned());
                } else {
                    kind = TokenKind::Command(name.to_owned());
                }
            }
        } else if exclusions
            && ch == '-'
            && raw[end..].starts_with(|c: char| c.is_ascii_alphabetic())
        {
            while end < raw.len()
                && (raw.as_bytes()[end].is_ascii_alphanumeric()
                    || matches!(raw.as_bytes()[end], b'_' | b'-'))
            {
                end += 1;
            }
            kind = TokenKind::Exclusion(raw[i + 1..end].to_owned());
        }
        exclusions = match &kind {
            TokenKind::Mention(name) if name == "all" => true,
            TokenKind::Exclusion(_) => true,
            TokenKind::Text if ch.is_whitespace() => exclusions,
            _ => false,
        };
        tokens.push(Token {
            kind,
            span: Span { start, end },
        });
        i = end;
    }
    Ok(tokens)
}
