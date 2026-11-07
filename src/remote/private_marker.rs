//! Bounded capability-marker framing before command output is visible to consumers.
//! Login banners need not end with a newline; capability bytes are always consumed privately.
pub(super) struct Filter {
    prefix: &'static [u8],
    pending: Vec<u8>,
    discard: bool,
    accepted: bool,
}
impl Filter {
    pub(super) fn new(prefix: &'static [u8]) -> Self {
        Self {
            prefix,
            pending: Vec::new(),
            discard: false,
            accepted: false,
        }
    }
    fn marker(&self) -> Option<usize> {
        self.pending
            .windows(self.prefix.len())
            .position(|p| p == self.prefix)
    }
    pub(super) fn feed(&mut self, bytes: &[u8], mut accept: impl FnMut(&[u8]) -> bool) -> Vec<u8> {
        if self.accepted {
            return bytes.to_vec();
        }
        self.pending.extend_from_slice(bytes);
        let mut output = Vec::new();
        while let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
            if self.discard {
                self.pending.drain(..=end);
                self.discard = false;
                continue;
            }
            if let Some(start) = self.marker().filter(|&start| start <= end) {
                output.extend(self.pending.drain(..start));
                let end = end - start;
                let line: Vec<_> = self.pending.drain(..=end).collect();
                if accept(&line[self.prefix.len()..]) {
                    self.accepted = true;
                    output.append(&mut self.pending);
                    return output;
                }
                output.extend_from_slice(b"gitcraft: invalid ownership handshake\n");
            } else {
                output.extend(self.pending.drain(..=end));
            }
        }
        if self.pending.len() > 4096 {
            if self.discard {
                self.pending.clear();
            } else if let Some(start) = self.marker() {
                output.extend(self.pending.drain(..start));
                self.pending.clear();
                self.discard = true;
                output.extend_from_slice(b"gitcraft: oversized ownership handshake\n");
            } else {
                // Retain a split prefix while bounding unrelated preamble buffering.
                let count = self.pending.len().saturating_sub(self.prefix.len() - 1);
                output.extend(self.pending.drain(..count));
            }
        }
        output
    }
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    const PREFIX: &[u8] = b"__OWNER__ ";
    #[test]
    fn banners_and_every_split_keep_capability_private() {
        let input = b"banner without newline__OWNER__ private-capability\ncommand\n";
        for split in 0..=input.len() {
            let mut filter = Filter::new(PREFIX);
            let accept = |b: &[u8]| b == b"private-capability\n";
            let mut output = filter.feed(&input[..split], accept);
            output.extend(filter.feed(&input[split..], accept));
            assert_eq!(output, b"banner without newlinecommand\n");
            assert_eq!(
                filter.feed(b"__OWNER__ user-output\n", accept),
                b"__OWNER__ user-output\n"
            );
        }
    }
    #[test]
    fn oversized_and_invalid_capability_frames_never_enter_output() {
        let mut filter = Filter::new(PREFIX);
        let mut input = PREFIX.to_vec();
        input.extend(vec![b'x'; 8000]);
        let first = filter.feed(&input, |_| false);
        assert_eq!(first, b"gitcraft: oversized ownership handshake\n");
        assert_eq!(
            filter.feed(b"secret-tail\n__OWNER__ wrong-token\n", |_| false),
            b"gitcraft: invalid ownership handshake\n"
        );
        assert!(filter.pending.len() <= 4096);
    }
}
