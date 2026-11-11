//! Generation-bound transfer bundles, receipts, and chunk scripts.
use super::*;

#[derive(Debug)]
pub(super) enum TransferFailure {
    Git(String),
    Operation(String),
}

impl TransferFailure {
    pub(super) fn message(self) -> String {
        match self {
            Self::Git(s) | Self::Operation(s) => s,
        }
    }
}

pub(super) struct TransferBundle {
    pub(super) nonce: String,
    pub(super) refs: Vec<(String, String)>,
    pub(super) bytes: Vec<u8>,
}

impl TransferBundle {
    pub(super) fn header(
        with: &[String],
        without: &[String],
        generation: &str,
        nonce: &str,
    ) -> Result<Self, String> {
        let valid_hex = |s: &str, size: usize| {
            s.len() == size
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        let width = with.first().map_or(0, String::len);
        if with.is_empty()
            || with.len() + without.len() > 4096
            || !matches!(width, 40 | 64)
            || !with.iter().chain(without).all(|h| valid_hex(h, width))
            || !valid_hex(generation, 64)
            || !valid_hex(nonce, 32)
        {
            return Err("invalid or excessive transfer commit identifiers".into());
        }
        let refs: Vec<_> = with
            .iter()
            .enumerate()
            .map(|(i, h)| {
                (
                    format!("refs/{FOLDER}/{generation}/sent-{nonce}-{i}"),
                    h.clone(),
                )
            })
            .collect();
        let mut header = if width == 64 {
            "# v3 git bundle\n@object-format=sha256\n".to_owned()
        } else {
            "# v2 git bundle\n".to_owned()
        };
        for hash in without {
            header.push_str(&format!("-{hash} gitcraft prerequisite\n"));
        }
        for (name, hash) in &refs {
            header.push_str(&format!("{hash} {name}\n"));
        }
        header.push('\n');
        Ok(Self {
            nonce: nonce.into(),
            refs,
            bytes: header.into_bytes(),
        })
    }
    pub(super) fn ref_transaction(&self) -> String {
        let mut input = String::from("start\n");
        for (name, hash) in &self.refs {
            input.push_str(&format!("create {name} {hash}\n"));
        }
        input.push_str("prepare\ncommit\n");
        input
    }
}

pub(super) fn random_transfer() -> Result<String, String> {
    let mut random = [0; 16];
    gitcraft_os::random_bytes(&mut random).map_err(|e| e.to_string())?;
    Ok(random.iter().map(|b| format!("{b:02x}")).collect())
}

pub(super) fn transfer_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Assemble the documented bundle header around read-only pack output. No source refs
/// are created, updated or removed, including failure/cancellation paths.
pub(super) fn bundle(
    source: &Path,
    with: &[String],
    without: &[String],
    generation: &str,
    nonce: &str,
) -> Result<TransferBundle, TransferFailure> {
    let mut bundle = TransferBundle::header(with, without, generation, nonce)
        .map_err(TransferFailure::Operation)?;
    let mut revisions = with.join("\n");
    revisions.push('\n');
    for hash in without {
        revisions.push_str(&format!("^{hash}\n"));
    }
    let pack = git::transfer_pack(
        source,
        revisions.into_bytes(),
        !without.is_empty(),
        MAX_SEND - bundle.bytes.len(),
    )
    .map_err(TransferFailure::Operation)?;
    if pack.code != Some(0) {
        return Err(TransferFailure::Git(format!(
            "could not pack transfer: {}",
            String::from_utf8_lossy(&pack.stderr)
        )));
    }
    if !pack.stdout.starts_with(b"PACK") || bundle.bytes.len() + pack.stdout.len() > MAX_SEND {
        return Err(TransferFailure::Operation("invalid pack or history exceeds 256 MiB transfer limit; clone the repository there and name its folder".into()));
    }
    bundle.bytes.extend(pack.stdout);
    Ok(bundle)
}

pub(super) const WRITE_CHUNK: &str = r#"set -eu
f=$1; offset=$2; end=$3
[ ! -L "$f" ] || exit 1
if [ "$offset" = 0 ]; then (umask 077; set -C; : >"$f") || exit 1; fi
[ -f "$f" ] && [ "$(wc -c <"$f" | tr -d '[:space:]')" = "$offset" ] || exit 1
cat >>"$f" || exit 1
[ "$(wc -c <"$f" | tr -d '[:space:]')" = "$end" ] || exit 1
"#;

pub(super) const VERIFY_TRANSFER: &str = r#"set -eu
[ ! -L "$1" ] && [ -f "$1" ] && [ "$(wc -c <"$1" | tr -d '[:space:]')" = "$2" ] || exit 1
if command -v sha256sum >/dev/null 2>&1; then digest=$(sha256sum "$1")
elif command -v shasum >/dev/null 2>&1; then digest=$(shasum -a 256 "$1")
elif command -v python3 >/dev/null 2>&1; then digest=$(python3 -c 'import hashlib,sys; h=hashlib.sha256(); f=open(sys.argv[1],"rb"); exec("while True:\n b=f.read(1048576)\n if not b: break\n h.update(b)\n"); print(h.hexdigest())' "$1")
else digest=$(perl -MDigest::SHA -e 'open(my $f,"<",$ARGV[0]) or die $!; binmode($f); print Digest::SHA->new(256)->addfile($f)->hexdigest' "$1"); fi
case "$digest" in "$3"|"$3 "*) ;; *) printf '%s\n' 'transferred bundle digest differs' >&2; exit 1;; esac
"#;

pub(super) const REMOVE_TRANSFER: &str = r#"set -eu
[ ! -L "$1" ] || exit 1
if [ -e "$1" ]; then [ -f "$1" ] && rm -- "$1" || exit 1; fi
[ ! -e "$1" ] && [ ! -L "$1" ]
"#;
