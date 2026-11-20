//! A canonical, bounded form of a self-relative Windows security descriptor.
//!
//! It is the form `RawSecurityDescriptor.GetBinaryForm` writes (owner, group, SACL, DACL,
//! each ACL's size recomputed) after clearing the inheritance status bits the resource
//! manager recalculates on an otherwise identical apply. ACE bytes are kept as stored.
//! Pure byte parsing: no OS calls, so it is checked on every platform.

/// Longest descriptor a snapshot accepts.
pub const DESCRIPTOR_LIMIT: usize = 1024 * 1024;

const HEADER: usize = 20;
const SELF_RELATIVE: u16 = 0x8000;
const DACL_PRESENT: u16 = 0x0004;
const SACL_PRESENT: u16 = 0x0010;
/// DACL/SACL auto-inherit-required and auto-inherited.
const INHERITANCE_STATUS: u16 = 0x0100 | 0x0200 | 0x0400 | 0x0800;
pub(crate) const DACL_PROTECTED: u16 = 0x1000;
pub(crate) const SACL_PROTECTED: u16 = 0x2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorError {
    /// Empty or over [`DESCRIPTOR_LIMIT`].
    TooLarge,
    /// Not a well-formed self-relative descriptor.
    Malformed,
}

/// The canonical descriptor and its control flags (inheritance status cleared).
pub fn canonical(raw: &[u8]) -> Result<(Vec<u8>, u16), DescriptorError> {
    if raw.is_empty() || raw.len() > DESCRIPTOR_LIMIT {
        return Err(DescriptorError::TooLarge);
    }
    if raw.len() < HEADER || raw[0] != 1 {
        return Err(DescriptorError::Malformed);
    }
    let control = u16_at(raw, 2)?;
    if control & SELF_RELATIVE == 0 {
        return Err(DescriptorError::Malformed);
    }
    let owner = sid(raw, u32_at(raw, 4)?)?;
    let group = sid(raw, u32_at(raw, 8)?)?;
    let sacl = acl(raw, u32_at(raw, 12)?, control & SACL_PRESENT != 0)?;
    let dacl = acl(raw, u32_at(raw, 16)?, control & DACL_PRESENT != 0)?;
    let control = control & !INHERITANCE_STATUS;
    let mut out = Vec::with_capacity(raw.len());
    out.extend_from_slice(&[1, raw[1]]);
    out.extend_from_slice(&control.to_le_bytes());
    out.extend_from_slice(&[0; 16]);
    for (slot, part) in [owner, group].into_iter().enumerate() {
        place(&mut out, 4 + slot * 4, part.as_deref())?;
    }
    for (slot, part) in [sacl, dacl].into_iter().enumerate() {
        place(&mut out, 12 + slot * 4, part.as_deref())?;
    }
    Ok((out, control))
}

/// What a policy snapshot holds: the captured scope, then the canonical descriptor. An
/// omitted SACL is thereby never equal to an empty one.
pub fn snapshot_bytes(info: u32, raw: &[u8]) -> Result<Vec<u8>, DescriptorError> {
    let (canonical, _) = canonical(raw)?;
    let mut snapshot = info.to_le_bytes().to_vec();
    snapshot.extend_from_slice(&canonical);
    Ok(snapshot)
}

fn place(out: &mut Vec<u8>, slot: usize, part: Option<&[u8]>) -> Result<(), DescriptorError> {
    let Some(part) = part else {
        return Ok(());
    };
    let at = u32::try_from(out.len()).map_err(|_| DescriptorError::TooLarge)?;
    out[slot..slot + 4].copy_from_slice(&at.to_le_bytes());
    out.extend_from_slice(part);
    Ok(())
}

/// `at + len`, refusing an offset past the address space as malformed.
fn end(at: usize, len: usize) -> Result<usize, DescriptorError> {
    at.checked_add(len).ok_or(DescriptorError::Malformed)
}

fn u16_at(raw: &[u8], at: usize) -> Result<u16, DescriptorError> {
    raw.get(at..end(at, 2)?)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or(DescriptorError::Malformed)
}

fn u32_at(raw: &[u8], at: usize) -> Result<usize, DescriptorError> {
    raw.get(at..end(at, 4)?)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .ok_or(DescriptorError::Malformed)
}

fn sid(raw: &[u8], at: usize) -> Result<Option<Vec<u8>>, DescriptorError> {
    if at == 0 {
        return Ok(None);
    }
    let head = raw.get(at..end(at, 8)?).ok_or(DescriptorError::Malformed)?;
    if head[0] != 1 || head[1] > 15 {
        return Err(DescriptorError::Malformed);
    }
    let len = 8 + 4 * usize::from(head[1]);
    raw.get(at..end(at, len)?)
        .map(|b| Some(b.to_vec()))
        .ok_or(DescriptorError::Malformed)
}

/// An ACL with its size recomputed from its ACEs (stored slack is not policy).
fn acl(raw: &[u8], at: usize, present: bool) -> Result<Option<Vec<u8>>, DescriptorError> {
    if !present || at == 0 {
        return Ok(None);
    }
    let size = usize::from(u16_at(raw, end(at, 2)?)?);
    let count = usize::from(u16_at(raw, end(at, 4)?)?);
    if size < 8 {
        return Err(DescriptorError::Malformed);
    }
    let body = raw
        .get(at..end(at, size)?)
        .ok_or(DescriptorError::Malformed)?;
    let mut aces = Vec::new();
    let mut next = 8;
    for _ in 0..count {
        let ace_size = usize::from(u16_at(body, end(next, 2)?)?);
        if ace_size < 4 || !ace_size.is_multiple_of(4) {
            return Err(DescriptorError::Malformed);
        }
        aces.extend_from_slice(
            body.get(next..end(next, ace_size)?)
                .ok_or(DescriptorError::Malformed)?,
        );
        next = end(next, ace_size)?;
    }
    let total = u16::try_from(8 + aces.len()).map_err(|_| DescriptorError::Malformed)?;
    let mut out = vec![body[0], 0];
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(
        &u16::try_from(count)
            .map_err(|_| DescriptorError::Malformed)?
            .to_le_bytes(),
    );
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&aces);
    Ok(Some(out))
}

#[cfg(test)]
#[path = "security_descriptor_tests.rs"]
mod tests;
