//! Explicit clipboard requests. The caller writes the returned bytes to its terminal.
use crate::markdown::{MarkdownRenderer, sanitize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyTarget {
    Message,
    CodeBlock(usize),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyRequest {
    pub sequence: String,
    pub feedback: &'static str,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyError {
    Unavailable,
    MissingCodeBlock,
}
impl CopyError {
    pub fn feedback(self) -> &'static str {
        match self {
            Self::Unavailable => "Copy not confirmed. Use /export to save the content.",
            Self::MissingCodeBlock => "No code block at this position.",
        }
    }
}
/// Call only from a user copy action. OSC 52 has no portable acknowledgement;
/// generating a request must never be presented as confirmed clipboard delivery.
pub fn request_copy(
    source: &str,
    target: CopyTarget,
    available: bool,
) -> Result<CopyRequest, CopyError> {
    if !available {
        return Err(CopyError::Unavailable);
    }
    let text = match target {
        CopyTarget::Message => sanitize(source),
        CopyTarget::CodeBlock(index) => {
            let mut markdown = MarkdownRenderer::new();
            markdown.update("copy", source, true, 80);
            markdown
                .blocks()
                .iter()
                .filter_map(|block| block.code())
                .nth(index)
                .ok_or(CopyError::MissingCodeBlock)?
                .to_owned()
        }
    };
    Ok(CopyRequest {
        sequence: format!("\x1b]52;c;{}\x07", base64(text.as_bytes())),
        feedback: "Copy requested; delivery not confirmed. Use /export if blocked.",
    })
}
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as usize;
        let b = chunk.get(1).copied().unwrap_or(0) as usize;
        let c = chunk.get(2).copied().unwrap_or(0) as usize;
        out.push(ALPHABET[a >> 2] as char);
        out.push(ALPHABET[(a & 3) << 4 | b >> 4] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(b & 15) << 2 | c >> 6] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[c & 63] as char
        } else {
            '='
        });
    }
    out
}
