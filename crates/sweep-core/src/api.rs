//! Read-only OS access traits. Stage A defines no mutating operations.

use crate::model::{RegRoot, RegValue, RegView};
use crate::winpath::WinPath;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FsError {
    NotFound,
    AccessDenied,
    /// The path resolves somewhere else: an ancestor is a junction/symlink
    /// or mount point (the resolved location is given). Never followed.
    Redirected(String),
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryMeta {
    pub is_dir: bool,
    /// Junction, symlink or other reparse point. Never followed.
    pub is_reparse: bool,
    pub size: u64,
    /// Last write time, seconds since the Unix epoch.
    pub modified: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub meta: EntryMeta,
}

/// Read-only filesystem access. Implementations must not follow reparse
/// points when producing `EntryMeta` (i.e. use lstat-like semantics).
pub trait FsRead: Sync {
    fn metadata(&self, path: &WinPath) -> Result<EntryMeta, FsError>;
    fn read_dir(&self, path: &WinPath) -> Result<Vec<DirEntry>, FsError>;
    fn is_file(&self, path: &WinPath) -> bool {
        matches!(self.metadata(path), Ok(m) if !m.is_dir && !m.is_reparse)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegError {
    NotFound,
    AccessDenied,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyState {
    Present,
    Absent,
    Unreadable(String),
}

/// Read-only registry access with an explicit view on every call.
pub trait RegRead: Sync {
    fn subkeys(&self, root: RegRoot, view: RegView, path: &str) -> Result<Vec<String>, RegError>;
    fn values(&self, root: RegRoot, view: RegView, path: &str) -> Result<BTreeMap<String, RegValue>, RegError>;
    /// Whether a key exists. Unlike a boolean, this keeps "could not tell"
    /// distinct: an access failure must never be read as "absent".
    fn key_state(&self, root: RegRoot, view: RegView, path: &str) -> KeyState {
        match self.values(root, view, path) {
            Ok(_) => KeyState::Present,
            Err(RegError::NotFound) => KeyState::Absent,
            Err(RegError::AccessDenied) => KeyState::Unreadable("access denied".into()),
            Err(RegError::Other(e)) => KeyState::Unreadable(e),
        }
    }
}

/// Resolved Known Folders for the user Sweep runs as and for the machine.
/// Every field comes from `SHGetKnownFolderPath`/environment on Windows; no
/// path is assumed to live on `C:`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KnownFolders {
    pub windows: Option<WinPath>,
    pub system32: Option<WinPath>,
    pub syswow64: Option<WinPath>,
    pub program_files: Option<WinPath>,
    pub program_files_x86: Option<WinPath>,
    pub program_files_common: Option<WinPath>,
    pub program_files_common_x86: Option<WinPath>,
    pub program_data: Option<WinPath>,
    pub users: Option<WinPath>,
    pub user_profile: Option<WinPath>,
    pub public: Option<WinPath>,
    pub roaming_app_data: Option<WinPath>,
    pub local_app_data: Option<WinPath>,
    pub local_programs: Option<WinPath>,
    pub start_menu_programs: Option<WinPath>,
    pub common_start_menu_programs: Option<WinPath>,
    pub start_menu: Option<WinPath>,
    pub common_start_menu: Option<WinPath>,
    pub desktop: Option<WinPath>,
    pub common_desktop: Option<WinPath>,
    pub documents: Option<WinPath>,
    pub pictures: Option<WinPath>,
    pub music: Option<WinPath>,
    pub videos: Option<WinPath>,
    pub downloads: Option<WinPath>,
    pub one_drive: Option<WinPath>,
    pub temp: Option<WinPath>,
    /// Sweep's own install/data directories.
    pub sweep_dirs: Vec<WinPath>,
    /// Known Folders that exist but could not be resolved to a supported
    /// local path (name, reason), e.g. redirected to a network share. These
    /// are reported as coverage gaps; folders simply not defined on the
    /// system are not listed here.
    pub unresolved: Vec<(String, String)>,
}

impl KnownFolders {
    /// Folders whose contents are presumed to be user data.
    pub fn user_data_roots(&self) -> Vec<WinPath> {
        [&self.documents, &self.pictures, &self.music, &self.videos, &self.downloads, &self.desktop, &self.one_drive]
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }
}
