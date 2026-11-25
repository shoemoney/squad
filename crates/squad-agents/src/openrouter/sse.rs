use super::TransportError;

// Bound an unfinished event so a broken provider cannot grow memory forever.
const MAX_EVENT_BYTES: usize = 1_048_576;

#[derive(Debug, PartialEq)]
pub(super) struct SseEvent {
    pub data: String,
    pub is_error: bool,
}

#[derive(Default)]
pub(super) struct SseParser {
    line: Vec<u8>,
    data: Vec<String>,
    event_bytes: usize,
    previous_cr: bool,
    error_event: bool,
}
impl SseParser {
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, TransportError> {
        let mut events = Vec::new();
        for &byte in chunk {
            if self.previous_cr && byte == b'\n' {
                self.previous_cr = false;
                continue;
            }
            self.previous_cr = byte == b'\r';
            if byte == b'\n' || byte == b'\r' {
                let line = std::mem::take(&mut self.line);
                let line =
                    std::str::from_utf8(&line).map_err(|_| TransportError::MalformedEvent)?;
                if line.is_empty() {
                    if !self.data.is_empty() {
                        events.push(SseEvent {
                            data: self.data.join("\n"),
                            is_error: self.error_event,
                        });
                        self.data.clear();
                    }
                    self.event_bytes = 0;
                    self.error_event = false;
                } else if let Some(event) = line.strip_prefix("event:") {
                    self.error_event = event.trim() == "error";
                } else if let Some(data) = line.strip_prefix("data:") {
                    self.data
                        .push(data.strip_prefix(' ').unwrap_or(data).to_owned());
                }
            } else {
                self.line.push(byte);
                self.event_bytes += 1;
                if self.event_bytes > MAX_EVENT_BYTES {
                    return Err(TransportError::MalformedEvent);
                }
            }
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_byte_boundary_including_utf8_crlf_and_multiline() {
        let input =
            ": keepalive\r\nevent: message\r\ndata: héllo\r\ndata: world\r\n\r\ndata: [DONE]\n\n"
                .as_bytes();
        for split in 0..=input.len() {
            let mut parser = SseParser::default();
            let mut events = parser.push(&input[..split]).unwrap();
            events.extend(parser.push(&input[split..]).unwrap());
            assert_eq!(
                events.iter().map(|e| e.data.as_str()).collect::<Vec<_>>(),
                ["héllo\nworld", "[DONE]"]
            );
        }
        let mut parser = SseParser::default();
        let events: Vec<_> = input
            .iter()
            .flat_map(|b| parser.push(&[*b]).unwrap())
            .collect();
        assert_eq!(
            events.iter().map(|e| e.data.as_str()).collect::<Vec<_>>(),
            ["héllo\nworld", "[DONE]"]
        );
    }
    #[test]
    fn rejects_unbounded_event_and_invalid_utf8() {
        assert!(
            SseParser::default()
                .push(&vec![b'x'; MAX_EVENT_BYTES + 1])
                .is_err()
        );
        assert!(SseParser::default().push(b"data: \xff\n").is_err());
    }
}
