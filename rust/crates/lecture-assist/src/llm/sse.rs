//! Server-sent events: pull `data:` payloads out of a byte stream.

/// Collects bytes until whole lines arrive; network chunks may split a line, or a UTF-8
/// character, anywhere.
#[derive(Default)]
pub struct SseParser {
    pending: Vec<u8>,
}

impl SseParser {
    /// The payload of every `data:` line completed by `bytes`, without the end marker.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(bytes);
        let mut payloads = Vec::new();
        while let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.strip_prefix(' ').unwrap_or(data);
                if data != "[DONE]" {
                    payloads.push(data.to_string());
                }
            }
        }
        payloads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STREAM: &str = ": keep-alive\r\ndata: {\"a\":\"Привіт\"}\r\n\r\nevent: x\ndata:{\"b\":1}\n\ndata: [DONE]\n\n";

    #[test]
    fn whole_stream() {
        let mut parser = SseParser::default();
        assert_eq!(
            parser.push(STREAM.as_bytes()),
            [r#"{"a":"Привіт"}"#, r#"{"b":1}"#]
        );
    }

    #[test]
    fn split_anywhere_gives_the_same() {
        let bytes = STREAM.as_bytes();
        for size in 1..8 {
            let mut parser = SseParser::default();
            let out: Vec<String> = bytes.chunks(size).flat_map(|c| parser.push(c)).collect();
            assert_eq!(
                out,
                [r#"{"a":"Привіт"}"#, r#"{"b":1}"#],
                "chunk size {size}"
            );
        }
    }

    #[test]
    fn an_unfinished_line_waits() {
        let mut parser = SseParser::default();
        assert!(parser.push(b"data: hal").is_empty());
        assert_eq!(parser.push(b"f\n"), ["half"]);
    }
}
