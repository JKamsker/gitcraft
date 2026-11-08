//! Owned source witnesses. Capture at the original branch, retain raw identity, translate
//! only when painting. This module never recognizes messages from their text.

use super::{Ecosystem, display};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ToolFailure {
    Opaque(String),
    Missing {
        raw: String,
        program: String,
    },
    Run {
        raw: String,
        program: String,
        detail: String,
    },
    Timeout {
        raw: String,
        program: String,
        minutes: u64,
    },
    Failed {
        raw: String,
        program: String,
    },
}
impl ToolFailure {
    pub(crate) fn raw(&self) -> &str {
        match self {
            Self::Opaque(raw)
            | Self::Missing { raw, .. }
            | Self::Run { raw, .. }
            | Self::Timeout { raw, .. }
            | Self::Failed { raw, .. } => raw,
        }
    }
    pub(crate) fn into_raw(self) -> String {
        match self {
            Self::Opaque(raw)
            | Self::Missing { raw, .. }
            | Self::Run { raw, .. }
            | Self::Timeout { raw, .. }
            | Self::Failed { raw, .. } => raw,
        }
    }
    pub(super) fn borrowed(&self) -> display::ToolFailure<'_> {
        match self {
            Self::Opaque(raw) => display::ToolFailure::Opaque(raw),
            Self::Missing { program, .. } => display::ToolFailure::Missing { program },
            Self::Run {
                program, detail, ..
            } => display::ToolFailure::Run { program, detail },
            Self::Timeout {
                program, minutes, ..
            } => display::ToolFailure::Timeout {
                program,
                minutes: *minutes,
            },
            Self::Failed { program, .. } => display::ToolFailure::Failed { program },
        }
    }
    pub(crate) fn display(&self) -> String {
        self.borrowed().display()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CargoFailure {
    Metadata { raw: String, detail: String },
    Tool { raw: String, failure: ToolFailure },
}
impl CargoFailure {
    pub(super) fn tool(failure: ToolFailure) -> Self {
        Self::Tool {
            raw: format!("cargo: {}", failure.raw()),
            failure,
        }
    }
    pub(crate) fn raw(&self) -> &str {
        match self {
            Self::Metadata { raw, .. } | Self::Tool { raw, .. } => raw,
        }
    }
    pub(crate) fn into_raw(self) -> String {
        match self {
            Self::Metadata { raw, .. } | Self::Tool { raw, .. } => raw,
        }
    }
    pub(super) fn borrowed(&self) -> display::CargoFailure<'_> {
        match self {
            Self::Metadata { detail, .. } => display::CargoFailure::Metadata { detail },
            Self::Tool { failure, .. } => display::CargoFailure::Tool(failure.borrowed()),
        }
    }
    pub(crate) fn display(&self) -> String {
        self.borrowed().display()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PackageJsonFailure {
    raw: String,
    detail: String,
}
impl PackageJsonFailure {
    pub(super) fn new(detail: String) -> Self {
        Self {
            raw: format!("package.json: {detail}"),
            detail,
        }
    }
    pub(crate) fn raw(&self) -> &str {
        &self.raw
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    Opaque(String),
    Tool(ToolFailure),
    NoManifest {
        raw: String,
    },
    NpmNoPackage {
        raw: String,
    },
    NpmPackAnswer {
        raw: String,
        detail: String,
    },
    OtherRegistry {
        raw: String,
        registry: String,
        ecosystem: Ecosystem,
    },
    RegistryJson {
        raw: String,
        url: String,
        detail: String,
    },
    AskRegistry {
        raw: String,
        url: String,
        detail: String,
    },
    Download {
        raw: String,
        url: String,
        detail: String,
    },
    ArchiveTooBig {
        raw: String,
    },
    ArchiveGzip {
        raw: String,
        detail: String,
    },
    ArchiveDamaged {
        raw: String,
    },
    Packages {
        raw: String,
        cargo: Option<CargoFailure>,
        package_json: Option<PackageJsonFailure>,
    },
}
impl From<ToolFailure> for Failure {
    fn from(failure: ToolFailure) -> Self {
        match failure {
            ToolFailure::Opaque(raw) => Self::Opaque(raw),
            failure => Self::Tool(failure),
        }
    }
}
impl Failure {
    pub(crate) fn raw(&self) -> &str {
        match self {
            Self::Tool(failure) => failure.raw(),
            Self::Opaque(raw)
            | Self::NoManifest { raw }
            | Self::NpmNoPackage { raw }
            | Self::NpmPackAnswer { raw, .. }
            | Self::OtherRegistry { raw, .. }
            | Self::RegistryJson { raw, .. }
            | Self::AskRegistry { raw, .. }
            | Self::Download { raw, .. }
            | Self::ArchiveTooBig { raw }
            | Self::ArchiveGzip { raw, .. }
            | Self::ArchiveDamaged { raw }
            | Self::Packages { raw, .. } => raw,
        }
    }
    pub(crate) fn into_raw(self) -> String {
        match self {
            Self::Tool(failure) => failure.into_raw(),
            Self::Opaque(raw)
            | Self::NoManifest { raw }
            | Self::NpmNoPackage { raw }
            | Self::NpmPackAnswer { raw, .. }
            | Self::OtherRegistry { raw, .. }
            | Self::RegistryJson { raw, .. }
            | Self::AskRegistry { raw, .. }
            | Self::Download { raw, .. }
            | Self::ArchiveTooBig { raw }
            | Self::ArchiveGzip { raw, .. }
            | Self::ArchiveDamaged { raw }
            | Self::Packages { raw, .. } => raw,
        }
    }
    pub(crate) fn display(&self) -> String {
        let diagnostic = match self {
            Self::Opaque(raw) => display::Diagnostic::Opaque(raw),
            Self::Tool(failure) => return failure.display(),
            Self::NoManifest { .. } => display::Diagnostic::NoManifest,
            Self::NpmNoPackage { .. } => display::Diagnostic::NpmNoPackage,
            Self::NpmPackAnswer { detail, .. } => display::Diagnostic::NpmPackAnswer { detail },
            Self::OtherRegistry {
                registry,
                ecosystem,
                ..
            } => display::Diagnostic::OtherRegistry {
                registry,
                ecosystem: *ecosystem,
            },
            Self::RegistryJson { url, detail, .. } => {
                display::Diagnostic::RegistryJson { url, detail }
            }
            Self::AskRegistry { url, detail, .. } => {
                display::Diagnostic::AskRegistry { url, detail }
            }
            Self::Download { url, detail, .. } => display::Diagnostic::Download { url, detail },
            Self::ArchiveTooBig { .. } => display::Diagnostic::ArchiveTooBig,
            Self::ArchiveGzip { detail, .. } => display::Diagnostic::ArchiveGzip { detail },
            Self::ArchiveDamaged { .. } => display::Diagnostic::ArchiveDamaged,
            Self::Packages {
                cargo,
                package_json,
                ..
            } => display::Diagnostic::Packages {
                cargo: cargo.as_ref().map(CargoFailure::borrowed),
                package_json: package_json.as_ref().map(|e| e.detail.as_str()),
            },
        };
        diagnostic.display()
    }
}
