//! SSE is framed by blank lines, NOT network chunks or UTF-8 byte boundaries.
use anyhow::{Result, ensure};

#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
    data: Vec<u8>,
}

impl Decoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<String>> {
        self.pending.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = self.pending.drain(..=end).collect();
            let mut line = &line[..line.len() - 1];
            if line.ends_with(b"\r") {
                line = &line[..line.len() - 1];
            }
            if line.is_empty() && !self.data.is_empty() {
                self.data.pop(); // The last data line added a newline.
                events.push(String::from_utf8(std::mem::take(&mut self.data))?);
            } else if let Some(value) = line.strip_prefix(b"data:") {
                let value = value.strip_prefix(b" ").unwrap_or(value);
                self.data.extend_from_slice(value);
                self.data.push(b'\n');
            }
            ensure!(self.data.len() <= 4_194_304, "SSE event exceeds 4 MiB");
        }
        ensure!(self.pending.len() <= 4_194_304, "SSE line exceeds 4 MiB");
        Ok(events)
    }
}
