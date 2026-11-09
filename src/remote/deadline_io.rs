//! Bounded nonblocking protocol I/O with one hard budget across partial operations.
use std::io::{self, Read, Write};

pub(super) fn read_frame(
    reader: &mut impl Read,
    max: usize,
    mut expired: impl FnMut() -> bool,
    mut pause: impl FnMut(),
) -> io::Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut bytes = [0u8; 256];
    loop {
        budget(&mut expired)?;
        let count = bytes
            .len()
            .min(max.saturating_add(1).saturating_sub(result.len()));
        if count == 0 {
            return Err(io::Error::other("oversized protocol frame"));
        }
        match reader.read(&mut bytes[..count]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "incomplete protocol frame",
                ));
            }
            Ok(n) => {
                result.extend_from_slice(&bytes[..n]);
                if result.len() > max {
                    return Err(io::Error::other("oversized protocol frame"));
                }
                if result.contains(&b'\n') {
                    budget(&mut expired)?;
                    return Ok(result);
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                pause()
            }
            Err(e) => return Err(e),
        }
    }
}
pub(super) fn write_frame(
    writer: &mut impl Write,
    bytes: &[u8],
    mut expired: impl FnMut() -> bool,
    mut pause: impl FnMut(),
) -> io::Result<()> {
    let mut sent = 0;
    while sent < bytes.len() {
        budget(&mut expired)?;
        match writer.write(&bytes[sent..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "protocol write stopped",
                ));
            }
            Ok(n) => sent += n,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                pause()
            }
            Err(e) => return Err(e),
        }
    }
    loop {
        budget(&mut expired)?;
        match writer.flush() {
            Ok(()) => return budget(&mut expired),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                pause()
            }
            Err(e) => return Err(e),
        }
    }
}
fn budget(expired: &mut impl FnMut() -> bool) -> io::Result<()> {
    if expired() {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "absolute protocol deadline expired",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    use std::cell::Cell;
    struct Drip<'a>(&'a Cell<usize>);
    impl Read for Drip<'_> {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            self.0.set(self.0.get() + 1);
            b[0] = b'x';
            Ok(1)
        }
    }
    struct Blocked<'a> {
        ticks: &'a Cell<usize>,
        sent: usize,
        flush: bool,
    }
    impl Write for Blocked<'_> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.ticks.set(self.ticks.get() + 1);
            if self.sent == 0 {
                self.sent += 1;
                Ok(b.len().min(1))
            } else {
                Err(io::ErrorKind::WouldBlock.into())
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            self.flush = true;
            self.ticks.set(self.ticks.get() + 1);
            Err(io::ErrorKind::WouldBlock.into())
        }
    }
    #[test]
    fn drip_reads_partial_writes_and_flush_cannot_reset_budget() {
        let ticks = Cell::new(0);
        assert_eq!(
            read_frame(&mut Drip(&ticks), 4096, || ticks.get() >= 7, || {})
                .err()
                .map(|e| e.kind()),
            Some(io::ErrorKind::TimedOut)
        );
        assert_eq!(ticks.get(), 7);
        ticks.set(0);
        let mut writer = Blocked {
            ticks: &ticks,
            sent: 0,
            flush: false,
        };
        assert_eq!(
            write_frame(&mut writer, b"abcdef", || ticks.get() >= 7, || {})
                .err()
                .map(|e| e.kind()),
            Some(io::ErrorKind::TimedOut)
        );
        assert_eq!(writer.sent, 1);
        ticks.set(0);
        let mut writer = Blocked {
            ticks: &ticks,
            sent: 0,
            flush: false,
        };
        assert!(write_frame(&mut writer, b"a", || ticks.get() >= 7, || {}).is_err());
        assert!(writer.flush);
        assert_eq!(ticks.get(), 7);
    }
}
