/// Nasdaq distributes ITCH files as a stream of messages, each prefixed with a
/// 2-byte big-endian length field (the payload length, excluding the prefix).
pub const LENGTH_PREFIX_BYTES: usize = 2;

#[allow(dead_code)]
pub fn load(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
}

/// Rewrites the `stock_locate` field (bytes 1..3 of every message) to a single
/// value, collapsing the whole file into one symbol's order flow.
pub fn normalize_locate(buf: &mut [u8], locate: u16) {
    let locate = locate.to_be_bytes();
    let mut pos = 0;
    while pos + LENGTH_PREFIX_BYTES <= buf.len() {
        let len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
        if len == 0 || pos + LENGTH_PREFIX_BYTES + len > buf.len() {
            return;
        }
        let msg = &mut buf[pos + LENGTH_PREFIX_BYTES..pos + LENGTH_PREFIX_BYTES + len];
        msg[1..3].copy_from_slice(&locate);
        pos += LENGTH_PREFIX_BYTES + len;
    }
}

pub fn count_messages(buf: &[u8]) -> usize {
    Splitter::new(buf).count()
}

/// Returns the distinct `stock_locate` values in the buffer, in first-seen order.
/// Only counts message types the engine dispatches (A/F/C/E/D/X/U/S) so the
/// number of books matches what the replay would create.
#[allow(dead_code)]
pub fn distinct_locates(buf: &[u8]) -> Vec<usize> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for msg in Splitter::new(buf) {
        if !matches!(msg[0], b'A' | b'F' | b'C' | b'E' | b'D' | b'X' | b'U' | b'S') {
            continue;
        }
        let locate = u16::from_be_bytes([msg[1], msg[2]]) as usize;
        if seen.insert(locate) {
            out.push(locate);
        }
    }
    out
}

pub struct Splitter<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Splitter<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
}

impl<'a> Iterator for Splitter<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos + LENGTH_PREFIX_BYTES > self.buf.len() {
            return None;
        }
        let len = u16::from_be_bytes([self.buf[self.pos], self.buf[self.pos + 1]]) as usize;
        if len == 0 || self.pos + LENGTH_PREFIX_BYTES + len > self.buf.len() {
            return None;
        }
        let msg = &self.buf[self.pos + LENGTH_PREFIX_BYTES..self.pos + LENGTH_PREFIX_BYTES + len];
        self.pos += LENGTH_PREFIX_BYTES + len;
        Some(msg)
    }
}
