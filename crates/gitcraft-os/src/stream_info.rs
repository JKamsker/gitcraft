//! Parse `FILE_STREAM_INFO` records (`GetFileInformationByHandleEx(FileStreamInfo)`).
//! Pure byte parsing, so it is checked on every platform.
// Elsewhere only its tests use it.
#![cfg_attr(not(windows), allow(dead_code))]

/// Offset of `StreamName` in a record: NextEntryOffset, StreamNameLength (u32 each), then
/// StreamSize and StreamAllocationSize (i64 each).
const NAME_AT: usize = 24;

/// Whether the records name only the unnamed data stream. Malformed records are `None`.
pub(crate) fn only_unnamed_data(records: &[u8]) -> Option<bool> {
    let mut at = 0usize;
    loop {
        let record = records.get(at..)?;
        let next = u32_at(record, 0)? as usize;
        let length = u32_at(record, 4)? as usize;
        let name = record.get(NAME_AT..NAME_AT.checked_add(length)?)?;
        if !length.is_multiple_of(2) {
            return None;
        }
        let units: Vec<u16> = name
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .take_while(|&u| u != 0)
            .collect();
        if !matches!(
            String::from_utf16(&units).ok()?.as_str(),
            "::$DATA" | ":$DATA" | "$DATA"
        ) {
            return Some(false);
        }
        if next == 0 {
            return Some(true);
        }
        if next < NAME_AT {
            return None;
        }
        at = at.checked_add(next)?;
    }
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(name: &str, last: bool, pad: usize) -> Vec<u8> {
        let name: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let size = (NAME_AT + name.len() + pad).next_multiple_of(8);
        let mut out = vec![0; size];
        let next = if last { 0 } else { size as u32 };
        out[0..4].copy_from_slice(&next.to_le_bytes());
        out[4..8].copy_from_slice(&(name.len() as u32).to_le_bytes());
        out[NAME_AT..NAME_AT + name.len()].copy_from_slice(&name);
        out
    }

    #[test]
    fn only_the_unnamed_data_stream_passes() {
        assert_eq!(only_unnamed_data(&record("::$DATA", true, 0)), Some(true));
        let two = [record("::$DATA", false, 0), record(":extra:$DATA", true, 0)].concat();
        assert_eq!(only_unnamed_data(&two), Some(false));
        let first = [record(":zone:$DATA", false, 0), record("::$DATA", true, 0)].concat();
        assert_eq!(only_unnamed_data(&first), Some(false));
    }

    #[test]
    fn a_name_is_cut_at_its_first_nul_and_malformed_records_are_refused() {
        assert_eq!(
            only_unnamed_data(&record("::$DATA\0junk", true, 0)),
            Some(true)
        );
        assert_eq!(
            only_unnamed_data(&record("\0::$DATA", true, 0)),
            Some(false)
        );
        let mut long = record("::$DATA", true, 0);
        long[4..8].copy_from_slice(&4096u32.to_le_bytes());
        assert_eq!(only_unnamed_data(&long), None);
        let mut looped = record("::$DATA", true, 0);
        looped[0..4].copy_from_slice(&4u32.to_le_bytes());
        assert_eq!(only_unnamed_data(&looped), None);
        assert_eq!(only_unnamed_data(&[0; 8]), None);
    }
}
