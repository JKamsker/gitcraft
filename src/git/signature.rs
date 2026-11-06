//! A commit's signature: whether it has one, of which kind, and what checking it says. Checked
//! only for the commit shown in the detail pane: history is read with checks off (see
//! [`super::NO_SIGNATURE`]), since every check runs gpg, ssh-keygen or gpgsm.

use std::path::Path;

use super::{END_OPTS, NO_SIGNATURE, UTF8, run};

/// The kind of key a commit is signed with, as its signature reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureFormat {
    Gpg,
    Ssh,
    X509,
    /// A signature of a kind gitcraft doesn't know.
    Other,
}

impl SignatureFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Gpg => "GPG",
            Self::Ssh => "SSH",
            Self::X509 => "X.509",
            Self::Other => "unknown kind",
        }
    }

    /// The kind of the armored signature `text` (its first line tells).
    fn of(text: &str) -> Self {
        let first = text.lines().next().unwrap_or_default().trim();
        match first {
            "-----BEGIN PGP SIGNATURE-----" | "-----BEGIN PGP MESSAGE-----" => Self::Gpg,
            "-----BEGIN SSH SIGNATURE-----" => Self::Ssh,
            "-----BEGIN SIGNED MESSAGE-----" | "-----BEGIN PKCS7-----" | "-----BEGIN CMS-----" => {
                Self::X509
            }
            _ => Self::Other,
        }
    }
}

/// What checking a signature found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// A good signature. `trusted`: by a key the keyring trusts (always so for SSH and X.509,
    /// whose allowed signers or certificates are the trust).
    Valid { trusted: bool },
    /// A good signature that has expired, or made with a key that since expired.
    Expired { key: bool },
    /// A good signature made with a key since revoked.
    Revoked,
    /// The signature doesn't match the commit: it was changed after signing, or it is forged.
    Invalid,
    /// The key it was made with isn't known here (not in the keyring, not an allowed signer).
    UnknownKey,
    /// The commit has no signature.
    Unsigned,
    /// It couldn't be checked: no program to check it with, nothing set up to check it against.
    NotChecked,
}

impl Verdict {
    /// The verdict in a few words, for the section's header.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Valid { trusted: true } => "Valid",
            Self::Valid { trusted: false } => "Valid, key not trusted",
            Self::Expired { key: false } => "Expired",
            Self::Expired { key: true } => "Valid, key expired",
            Self::Revoked => "Key revoked",
            Self::Invalid => "Invalid",
            Self::UnknownKey => "Unknown key",
            Self::Unsigned => "Unsigned",
            Self::NotChecked => "Not checked",
        }
    }

    /// What it means, in a sentence.
    pub fn explanation(&self) -> &'static str {
        match self {
            Self::Valid { trusted: true } => "The signature is good and its key is trusted.",
            Self::Valid { trusted: false } => {
                "The signature is good, but the key isn't marked as trusted in your keyring."
            }
            Self::Expired { key: false } => "The signature is good, but it has expired.",
            Self::Expired { key: true } => {
                "The signature is good, but the key it was made with has expired."
            }
            Self::Revoked => "The signature was made with a key that has since been revoked.",
            Self::Invalid => {
                "The signature does not match this commit: it was changed after signing, or the \
                 signature is not genuine."
            }
            Self::UnknownKey => {
                "The key this commit was signed with isn't known here: import it, or add it to \
                 your allowed signers file (gpg.ssh.allowedSignersFile) for SSH."
            }
            Self::Unsigned => "This commit has no signature.",
            Self::NotChecked => "The signature could not be checked; git's output says why.",
        }
    }
}

/// A commit's signature, checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub verdict: Verdict,
    /// `None` when unsigned.
    pub format: Option<SignatureFormat>,
    /// Who signed it, as the key or the allowed signers name them (may be empty).
    pub signer: String,
    /// The key it was made with: its ID or fingerprint.
    pub key: String,
    /// The fingerprint of the primary key, when it is a subkey's signature.
    pub primary_key: String,
    /// What the checking program said, as git gives it.
    pub raw: String,
}

/// Field separator in the `--format` below: no signer or key has a NUL.
const SEP: char = '\0';

/// Checks the signature of `hash`. Unsigned commits are told without running any program.
pub fn verify_signature(repo: &Path, hash: &str) -> Result<Signature, String> {
    let object = run(Some(repo), &["cat-file", "commit", hash])
        .map_err(|e| format!("Could not read commit {hash}: {e}"))?;
    let Some(signature) = signature_header(&object) else {
        return Ok(Signature {
            verdict: Verdict::Unsigned,
            format: None,
            signer: String::new(),
            key: String::new(),
            primary_key: String::new(),
            raw: String::new(),
        });
    };
    let format = SignatureFormat::of(&signature);
    let out = run(
        Some(repo),
        &[
            "show",
            // The check is asked for by the placeholders, not by showing it in the output.
            NO_SIGNATURE,
            UTF8,
            "-s",
            "--format=%G?%x00%GS%x00%GK%x00%GP%x00%GG",
            END_OPTS,
            hash,
        ],
    )
    .map_err(|e| format!("Could not check the signature: {e}"))?;
    let mut sig = parse_check(&out, format)?;
    if sig.raw.trim().is_empty() {
        // git gave no output of the checking program: it couldn't run it, or had nothing to
        // check against. `verify-commit` says why (on its error output).
        sig.raw = match run(Some(repo), &["verify-commit", "--raw", END_OPTS, hash]) {
            Ok(out) => out,
            Err(e) => e,
        };
        sig.verdict = unexplained(&sig.verdict);
    }
    Ok(sig)
}

/// The verdict to give when git said `verdict` but the checking program said nothing: git
/// says "bad" (`B`) when the program couldn't even be started, and "no signature" (`N`) when
/// there was nothing to check an SSH signature against.
fn unexplained(verdict: &Verdict) -> Verdict {
    match verdict {
        Verdict::Invalid | Verdict::Unsigned => Verdict::NotChecked,
        other => other.clone(),
    }
}

/// The signature in the headers of the commit object `object` (`gpgsig`, or `gpgsig-sha256`
/// in a SHA-256 repository), its continuation lines joined.
pub(super) fn signature_header(object: &str) -> Option<String> {
    let mut found: Option<String> = None;
    for line in object.lines() {
        if line.is_empty() {
            break; // The headers end; the message begins.
        }
        match (&mut found, line.strip_prefix(' ')) {
            (Some(sig), Some(rest)) => {
                sig.push('\n');
                sig.push_str(rest);
            }
            (Some(_), None) => break,
            (None, _) => {
                if let Some(rest) = line
                    .strip_prefix("gpgsig ")
                    .or_else(|| line.strip_prefix("gpgsig-sha256 "))
                {
                    found = Some(rest.to_owned());
                }
            }
        }
    }
    found
}

/// The signature of a signed commit, from what `--format=%G?…` printed for it.
pub(super) fn parse_check(out: &str, format: SignatureFormat) -> Result<Signature, String> {
    let mut fields = out.splitn(5, SEP);
    let mut next = || fields.next().map(str::to_owned);
    let (Some(code), Some(signer), Some(key), Some(primary_key), Some(raw)) =
        (next(), next(), next(), next(), next())
    else {
        return Err("Unexpected output from git while checking the signature".to_owned());
    };
    let raw = raw.trim_end().to_owned();
    let verdict = verdict(code.trim(), format, &signer, &raw);
    Ok(Signature {
        verdict,
        format: Some(format),
        signer,
        key,
        primary_key,
        raw,
    })
}

/// git's one-letter status (`%G?`) as a verdict.
fn verdict(code: &str, format: SignatureFormat, signer: &str, raw: &str) -> Verdict {
    match code {
        "G" => Verdict::Valid { trusted: true },
        // For SSH, a good signature by a key no allowed signer has ("No principal matched").
        "U" if format == SignatureFormat::Ssh && signer.is_empty() => Verdict::UnknownKey,
        "U" => Verdict::Valid { trusted: false },
        "X" => Verdict::Expired { key: false },
        "Y" => Verdict::Expired { key: true },
        "R" => Verdict::Revoked,
        "B" => Verdict::Invalid,
        "E" if says_missing_key(raw) => Verdict::UnknownKey,
        // Signed (the header is there), but git couldn't check it.
        _ => Verdict::NotChecked,
    }
}

/// Whether the checking program's output says it lacks the key.
fn says_missing_key(raw: &str) -> bool {
    let raw = raw.to_lowercase();
    [
        "no public key",
        "public key not found",
        "no principal",
        "certificate not found",
    ]
    .iter()
    .any(|s| raw.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_signature_is_read_from_the_headers_only() {
        let object = "tree 4b82\nauthor a <a> 1 +0000\ncommitter a <a> 1 +0000\ngpgsig -----BEGIN SSH SIGNATURE-----\n U1NIU0lH\n -----END SSH SIGNATURE-----\n\nmessage\ngpgsig fake\n";
        let sig = signature_header(object);
        assert_eq!(
            sig.as_deref(),
            Some("-----BEGIN SSH SIGNATURE-----\nU1NIU0lH\n-----END SSH SIGNATURE-----")
        );
        assert_eq!(
            SignatureFormat::of(&sig.unwrap_or_default()),
            SignatureFormat::Ssh
        );
        // A message that looks like a header is not one.
        assert_eq!(signature_header("tree x\n\ngpgsig -----BEGIN PGP\n"), None);
        assert_eq!(signature_header(""), None);
        // SHA-256 repositories.
        let sha256 = "tree x\ngpgsig-sha256 -----BEGIN PGP SIGNATURE-----\n x\n\nm";
        assert_eq!(
            SignatureFormat::of(&signature_header(sha256).unwrap_or_default()),
            SignatureFormat::Gpg
        );
    }

    #[test]
    fn signature_kinds_are_told_apart() {
        assert_eq!(
            SignatureFormat::of("-----BEGIN PGP SIGNATURE-----\nx"),
            SignatureFormat::Gpg
        );
        assert_eq!(
            SignatureFormat::of("-----BEGIN SIGNED MESSAGE-----"),
            SignatureFormat::X509
        );
        assert_eq!(SignatureFormat::of("garbage"), SignatureFormat::Other);
        assert_eq!(SignatureFormat::of(""), SignatureFormat::Other);
    }

    #[test]
    fn git_status_letters_become_verdicts() {
        let ssh = SignatureFormat::Ssh;
        let gpg = SignatureFormat::Gpg;
        let check = |out: &str, format| parse_check(out, format).map(|s| s.verdict);
        assert_eq!(
            check("G\0t@t\0SHA256:k\0SHA256:k\0Good \"git\" signature", ssh),
            Ok(Verdict::Valid { trusted: true })
        );
        assert_eq!(
            check("U\0\0SHA256:k\0\0No principal matched.", ssh),
            Ok(Verdict::UnknownKey)
        );
        assert_eq!(
            check("U\0Ann <a@a>\0ABCD\0\0Good signature", gpg),
            Ok(Verdict::Valid { trusted: false })
        );
        assert_eq!(check("B\0\0\0\0bad", ssh), Ok(Verdict::Invalid));
        assert_eq!(check("X\0\0\0\0", gpg), Ok(Verdict::Expired { key: false }));
        assert_eq!(check("Y\0\0\0\0", gpg), Ok(Verdict::Expired { key: true }));
        assert_eq!(check("R\0\0\0\0", gpg), Ok(Verdict::Revoked));
        assert_eq!(
            check("E\0\0ABCD\0\0Can't check signature: No public key", gpg),
            Ok(Verdict::UnknownKey)
        );
        assert_eq!(
            check("E\0\0\0\0other trouble", gpg),
            Ok(Verdict::NotChecked)
        );
        assert_eq!(check("N\0\0\0\0", ssh), Ok(Verdict::NotChecked));
        assert!(check("G", ssh).is_err());
        assert!(check("", ssh).is_err());
    }

    #[test]
    fn a_signer_with_unicode_and_a_multiline_output_are_kept() -> Result<(), String> {
        let sig = parse_check(
            "G\0Zoë Ünal <zoë@example.org>\0KEY\0PRIMARY\0line one\nline two\n\n",
            SignatureFormat::Gpg,
        )?;
        assert_eq!(sig.signer, "Zoë Ünal <zoë@example.org>");
        assert_eq!(sig.primary_key, "PRIMARY");
        assert_eq!(sig.raw, "line one\nline two");
        Ok(())
    }

    #[test]
    fn a_check_git_could_not_run_is_not_called_invalid() {
        assert_eq!(unexplained(&Verdict::Invalid), Verdict::NotChecked);
        assert_eq!(unexplained(&Verdict::Unsigned), Verdict::NotChecked);
        assert_eq!(
            unexplained(&Verdict::Valid { trusted: true }),
            Verdict::Valid { trusted: true }
        );
    }
}
