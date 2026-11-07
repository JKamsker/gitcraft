//! Public-only upload input: never forward a whole user-selected file to a forge.
use base64::{Engine, engine::general_purpose::STANDARD};
use std::io::Read;
use std::path::Path;

const BAD: &str = "The .pub companion must contain one valid OpenSSH public key. Private-key material, options and malformed input are refused.";
pub(super) const MAX_PUBLIC_BYTES: usize = 16384;

pub(crate) struct PublicKey(String);
impl PublicKey {
    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) fn read(path: &Path) -> Result<PublicKey, String> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(|_| "Could not open the selected key's .pub companion.".to_owned())?;
    if !meta.is_file() || meta.len() > MAX_PUBLIC_BYTES as u64 {
        return Err(BAD.into());
    }
    let file = std::fs::File::open(path).map_err(|_| BAD.to_owned())?;
    let mut bytes = Vec::new();
    file.take(MAX_PUBLIC_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| BAD.to_owned())?;
    parse(&bytes)
}

fn parse(bytes: &[u8]) -> Result<PublicKey, String> {
    if bytes.len() > MAX_PUBLIC_BYTES {
        return Err(BAD.into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| BAD.to_owned())?;
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let line = lines.next().ok_or(BAD)?;
    if lines.next().is_some() {
        return Err(BAD.into());
    }
    let mut words = line.split_whitespace();
    let kind = words.next().ok_or(BAD)?;
    let encoded = words.next().ok_or(BAD)?;
    if ![
        "ssh-ed25519",
        "ssh-rsa",
        "ecdsa-sha2-nistp256",
        "ecdsa-sha2-nistp384",
        "ecdsa-sha2-nistp521",
        "sk-ssh-ed25519@openssh.com",
        "sk-ecdsa-sha2-nistp256@openssh.com",
    ]
    .contains(&kind)
    {
        return Err(BAD.into());
    }
    let decoded = STANDARD.decode(encoded).map_err(|_| BAD.to_owned())?;
    let mut at = 0;
    if field(&decoded, &mut at)? != kind.as_bytes() {
        return Err(BAD.into());
    }
    match kind {
        "ssh-ed25519" | "sk-ssh-ed25519@openssh.com" => {
            if field(&decoded, &mut at)?.len() != 32 {
                return Err(BAD.into());
            }
        }
        "ssh-rsa" => {
            if field(&decoded, &mut at)?.is_empty() || field(&decoded, &mut at)?.is_empty() {
                return Err(BAD.into());
            }
        }
        _ => {
            let (curve, point_len) = if kind.contains("nistp256") {
                (b"nistp256".as_slice(), 65)
            } else if kind.contains("nistp384") {
                (b"nistp384".as_slice(), 97)
            } else {
                (b"nistp521".as_slice(), 133)
            };
            if field(&decoded, &mut at)? != curve {
                return Err(BAD.into());
            }
            let point = field(&decoded, &mut at)?;
            if point.len() != point_len || point.first() != Some(&4) {
                return Err(BAD.into());
            }
        }
    }
    if kind.starts_with("sk-") && field(&decoded, &mut at)?.is_empty() {
        return Err(BAD.into());
    }
    if at != decoded.len() {
        return Err(BAD.into());
    }
    // Ignore comments completely: an arbitrary comment is not public-key data.
    Ok(PublicKey(format!("{kind} {encoded}")))
}

fn field<'a>(bytes: &'a [u8], at: &mut usize) -> Result<&'a [u8], String> {
    let length_end = at.checked_add(4).ok_or(BAD)?;
    let length = bytes
        .get(*at..length_end)
        .and_then(|x| x.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or(BAD)? as usize;
    let end = length_end.checked_add(length).ok_or(BAD)?;
    let value = bytes.get(length_end..end).ok_or(BAD)?;
    *at = end;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> String {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&11u32.to_be_bytes());
        bytes.extend_from_slice(b"ssh-ed25519");
        bytes.extend_from_slice(&32u32.to_be_bytes());
        bytes.extend_from_slice(&[7; 32]);
        format!("ssh-ed25519 {}", STANDARD.encode(bytes))
    }
    #[test]
    fn only_public_key_fields_leave_the_validator() -> Result<(), String> {
        let expected = fixture();
        assert_eq!(
            parse(format!("{expected} ignored fixture comment\n").as_bytes())?.as_str(),
            expected
        );
        for text in ["-----BEGIN OPENSSH PRIVATE KEY-----\nfixture-private\n-----END OPENSSH PRIVATE KEY-----".to_owned(),format!("{expected}\nfixture-private"),format!("command=fixture {expected}"),"ssh-ed25519 fixture-private".into()] { let error=parse(text.as_bytes()).err().ok_or("Expected refusal")?; assert!(!error.contains("fixture-private")); }
        assert!(parse(&vec![b'x'; MAX_PUBLIC_BYTES + 1]).is_err());
        Ok(())
    }
}
