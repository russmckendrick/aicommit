//! Minimal Server-Sent Events parser for streaming provider responses.
//!
//! Bytes arrive in arbitrary chunks, so the parser buffers partial lines and
//! emits one `data` payload per complete event (multi-line `data:` fields are
//! joined with `\n`, per the SSE spec). `event:`, `id:`, and comment lines are
//! ignored: both OpenAI and Anthropic put everything needed in the payload.

#[derive(Debug, Default)]
pub struct SseParser {
    buffer: Vec<u8>,
    data: Vec<String>,
}

impl SseParser {
    /// Feed a chunk of bytes and return every event payload it completes.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=newline).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);
            self.handle_line(line, &mut events);
        }

        events
    }

    /// Flush a trailing event that was not followed by a blank line.
    pub fn finish(&mut self) -> Vec<String> {
        let mut events = Vec::new();
        if !self.buffer.is_empty() {
            let rest = std::mem::take(&mut self.buffer);
            let line = String::from_utf8_lossy(&rest).trim_end().to_owned();
            self.handle_line(&line, &mut events);
        }
        self.handle_line("", &mut events);
        events
    }

    fn handle_line(&mut self, line: &str, events: &mut Vec<String>) {
        if line.is_empty() {
            if !self.data.is_empty() {
                events.push(self.data.join("\n"));
                self.data.clear();
            }
            return;
        }
        if let Some(value) = line.strip_prefix("data:") {
            self.data
                .push(value.strip_prefix(' ').unwrap_or(value).to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SseParser;

    #[test]
    fn emits_events_split_across_chunks() {
        let mut parser = SseParser::default();
        assert!(parser.push(b"data: {\"a\":").is_empty());
        assert!(parser.push(b"1}\n").is_empty());
        assert_eq!(
            parser.push(b"\ndata: [DONE]\n\n"),
            vec!["{\"a\":1}", "[DONE]"]
        );
    }

    #[test]
    fn ignores_event_and_comment_lines_and_handles_crlf() {
        let mut parser = SseParser::default();
        let events =
            parser.push(b": keep-alive\r\nevent: content_block_delta\r\ndata: {\"x\":2}\r\n\r\n");
        assert_eq!(events, vec!["{\"x\":2}"]);
    }

    #[test]
    fn joins_multi_line_data_and_flushes_on_finish() {
        let mut parser = SseParser::default();
        assert!(parser.push(b"data: one\ndata: two\n").is_empty());
        assert_eq!(parser.finish(), vec!["one\ntwo"]);
    }

    #[test]
    fn keeps_multibyte_characters_split_across_chunks() {
        let mut parser = SseParser::default();
        let bytes = "data: ✨ feat\n\n".as_bytes();
        assert!(parser.push(&bytes[..8]).is_empty());
        assert_eq!(parser.push(&bytes[8..]), vec!["✨ feat"]);
    }
}
