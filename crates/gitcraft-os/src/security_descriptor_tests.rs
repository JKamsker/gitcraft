use super::*;

// S-1-5-32-544 (Administrators) and S-1-5-18 (SYSTEM).
const ADMINS: [u8; 16] = [1, 2, 0, 0, 0, 0, 0, 5, 32, 0, 0, 0, 32, 2, 0, 0];
const SYSTEM: [u8; 12] = [1, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0];

/// ACCESS_ALLOWED_ACE for SYSTEM with full control.
fn allow_system() -> Vec<u8> {
    let mut ace = vec![0, 0x10, 20, 0];
    ace.extend_from_slice(&0x001f_01ffu32.to_le_bytes());
    ace.extend_from_slice(&SYSTEM);
    ace
}

/// An ACL whose stored size has `slack` unused bytes after its ACEs.
fn acl_bytes(aces: &[Vec<u8>], slack: usize) -> Vec<u8> {
    let body: Vec<u8> = aces.concat();
    let size = (8 + body.len() + slack) as u16;
    let mut acl = vec![2, 0];
    acl.extend_from_slice(&size.to_le_bytes());
    acl.extend_from_slice(&(aces.len() as u16).to_le_bytes());
    acl.extend_from_slice(&[0, 0]);
    acl.extend_from_slice(&body);
    acl.extend(std::iter::repeat_n(0, slack));
    acl
}

/// Self-relative descriptor laid out DACL, group, owner (as stored descriptors may be).
fn stored(control: u16, slack: usize) -> Vec<u8> {
    let dacl = acl_bytes(&[allow_system()], slack);
    let mut raw = vec![1, 0];
    raw.extend_from_slice(&control.to_le_bytes());
    raw.extend_from_slice(&[0; 16]);
    let dacl_at = raw.len() as u32;
    raw.extend_from_slice(&dacl);
    let group_at = raw.len() as u32;
    raw.extend_from_slice(&SYSTEM);
    let owner_at = raw.len() as u32;
    raw.extend_from_slice(&ADMINS);
    raw[4..8].copy_from_slice(&owner_at.to_le_bytes());
    raw[8..12].copy_from_slice(&group_at.to_le_bytes());
    raw[16..20].copy_from_slice(&dacl_at.to_le_bytes());
    raw
}

#[test]
fn layout_slack_and_inheritance_status_do_not_change_the_canonical_form() {
    let plain = canonical(&stored(0x8004, 0)).expect("well formed");
    let inherited = canonical(&stored(0x8004 | 0x0400 | 0x0100, 12)).expect("well formed");
    assert_eq!(plain, inherited);
    let (bytes, control) = plain;
    assert_eq!(control, 0x8004);
    // Owner, group, then DACL, right after the header.
    assert_eq!(&bytes[4..8], &20u32.to_le_bytes());
    assert_eq!(&bytes[8..12], &36u32.to_le_bytes());
    assert_eq!(&bytes[12..16], &0u32.to_le_bytes());
    assert_eq!(&bytes[16..20], &48u32.to_le_bytes());
    assert_eq!(&bytes[20..36], &ADMINS);
    assert_eq!(&bytes[36..48], &SYSTEM);
    assert_eq!(&bytes[48..], acl_bytes(&[allow_system()], 0).as_slice());
}

#[test]
fn protection_aces_and_scope_remain_part_of_the_policy() {
    let open = snapshot_bytes(7, &stored(0x8004, 0)).expect("well formed");
    let protected = snapshot_bytes(7, &stored(0x8004 | DACL_PROTECTED, 0)).expect("well formed");
    assert_ne!(open, protected);
    assert_ne!(
        open,
        snapshot_bytes(0x1ff, &stored(0x8004, 0)).expect("well formed")
    );
    assert_eq!(&open[..4], &7u32.to_le_bytes());
    let mut other = stored(0x8004, 0);
    let mask_at = other.len() - ADMINS.len() - SYSTEM.len() - 20 + 4;
    other[mask_at] = 0xa9;
    assert_ne!(open, snapshot_bytes(7, &other).expect("well formed"));
}

#[test]
fn an_absent_dacl_flag_ignores_its_offset_and_a_null_dacl_stays_null() {
    let (absent, _) = canonical(&stored(0x8000, 0)).expect("well formed");
    assert_eq!(&absent[16..20], &0u32.to_le_bytes());
    assert_eq!(absent.len(), 20 + ADMINS.len() + SYSTEM.len());
    let mut null = stored(0x8004, 0);
    null[16..20].copy_from_slice(&0u32.to_le_bytes());
    let (null, control) = canonical(&null).expect("well formed");
    assert_eq!(control & DACL_PRESENT, DACL_PRESENT);
    assert_eq!(&null[16..20], &0u32.to_le_bytes());
}

#[test]
fn malformed_or_oversized_descriptors_are_refused() {
    assert_eq!(canonical(&[]), Err(DescriptorError::TooLarge));
    assert_eq!(
        canonical(&vec![1; DESCRIPTOR_LIMIT + 1]),
        Err(DescriptorError::TooLarge)
    );
    let good = stored(0x8004, 0);
    assert_eq!(canonical(&good[..19]), Err(DescriptorError::Malformed));
    let mut revision = good.clone();
    revision[0] = 2;
    assert_eq!(canonical(&revision), Err(DescriptorError::Malformed));
    let mut absolute = good.clone();
    absolute[3] = 0;
    assert_eq!(canonical(&absolute), Err(DescriptorError::Malformed));
    let mut owner = good.clone();
    owner[4..8].copy_from_slice(&(good.len() as u32 - 4).to_le_bytes());
    assert_eq!(canonical(&owner), Err(DescriptorError::Malformed));
    let mut count = good.clone();
    count[24] = 2;
    assert_eq!(canonical(&count), Err(DescriptorError::Malformed));
    let mut ace = good;
    ace[30] = 21;
    assert_eq!(canonical(&ace), Err(DescriptorError::Malformed));
}
