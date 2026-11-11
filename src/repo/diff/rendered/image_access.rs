use super::*;
#[derive(Default)]
pub(super) struct Images {
    pub(super) found: Vec<(String, Arc<[u8]>)>,
    uris: HashSet<String>,
    /// How many were looked for on the side being read.
    tried: usize,
}

impl Images {
    pub(super) fn uris(&self) -> &HashSet<String> {
        &self.uris
    }
    pub(super) fn into_images(self) -> Vec<(String, Arc<[u8]>)> {
        self.found
    }

    /// `text`, a side's file, made safe, with its images of the repository read as they are
    /// on that side.
    pub(super) fn safe_text(&mut self, repo: &Path, side: &Side, text: &str) -> String {
        self.tried = 0;
        let (text, _) = pr::safe_local_markdown(text, &mut |url| self.local(repo, side, url));
        text
    }

    /// The address to show the image at `url` from, when it is a file of the repository.
    pub(super) fn local(&mut self, repo: &Path, side: &Side, url: &str) -> Option<String> {
        let path = repo_path(&side.path, url)?;
        if self.tried >= MAX_IMAGES {
            return None;
        }
        self.tried += 1;
        let bytes = match side.rev {
            None => working_tree_image(repo, &path)?,
            Some(_) => git::file_at(repo, side.rev.as_deref(), &path).ok()?,
        };
        if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
            return None;
        }
        let uri = image_uri(&path, &bytes);
        if self.uris.insert(uri.clone()) {
            self.found
                .push((uri.clone(), Arc::from(bytes.into_boxed_slice())));
        }
        Some(uri)
    }
}

/// Named by what it holds, as the pictures of a diff are (see `images::image_id`); the name of
/// the file stays at the end, where the loaders look for its type.
pub(super) fn image_uri(path: &str, bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    let name: String = path
        .rsplit('/')
        .next()
        .unwrap_or("image")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("bytes://markdown/{:016x}/{name}", h.finish())
}

/// The file of the repository an image at `url` in the file `file` stands for: relative to the
/// file's folder, or to the repository's top with a leading `/`, as forges read it. Nothing for
/// an address elsewhere (with a scheme, a drive, a server…) or one leading out of the repository.
pub(super) fn repo_path(file: &str, url: &str) -> Option<String> {
    let url = url.split(['#', '?']).next().unwrap_or("");
    let url = percent_decoded(url)?;
    if url.is_empty() || url.contains(['\\', ':', '\0']) || url.starts_with("//") {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    if !url.starts_with('/') {
        parts.extend(file.split('/'));
        parts.pop();
    }
    for part in url.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// `text` with its `%xx` escapes decoded; nothing when that isn't UTF-8.
pub(super) fn percent_decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(hi), Some(lo)) = (
                bytes.get(i + 1).copied().and_then(hex),
                bytes.get(i + 2).copied().and_then(hex),
            )
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}
