//! In-memory fakes of the read-only OS traits, with Windows semantics:
//! case-insensitive lookup, junctions as reparse entries, per-path
//! access-denied flags. Paths are always Windows-style strings.

use crate::api::{DirEntry, EntryMeta, FsError, FsRead, KnownFolders, RegError, RegRead};
use crate::model::{RegRoot, RegValue, RegView};
use crate::winpath::{fold, WinPath};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

fn wp(s: &str) -> WinPath {
    WinPath::parse_external(s).expect("fake path")
}

type Hook = Box<dyn FnMut(&WinPath, &FakeFs) + Send>;

/// A junction/symlink: listed as a reparse point. Paths *through* it resolve
/// to `target` and are reported as `FsError::Redirected` (Windows behaviour
/// once the final path is checked), never silently followed.
#[derive(Default)]
pub struct FakeFs {
    nodes: Mutex<BTreeMap<String, (WinPath, EntryMeta, Option<String>)>>,
    denied: Mutex<BTreeSet<String>>,
    /// Extra listing entries whose exact names cannot be a `WinPath`
    /// (e.g. `data.`), keyed by parent directory.
    raw: Mutex<BTreeMap<String, Vec<DirEntry>>>,
    /// Hook run before every read_dir (used to cancel mid-scan in tests).
    pub on_read_dir: Mutex<Option<Hook>>,
    /// Hook run after every successful read_dir listing (used to change the
    /// tree between listing and inspection: replacement-race tests).
    pub after_read_dir: Mutex<Option<Hook>>,
}

fn dir_meta() -> EntryMeta {
    EntryMeta { is_dir: true, is_reparse: false, size: 0, modified: Some(0) }
}

impl FakeFs {
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(&self, p: WinPath, meta: EntryMeta, target: Option<String>) {
        let mut nodes = self.nodes.lock().unwrap();
        let mut cur = p.parent();
        while let Some(d) = cur {
            if d.is_root() {
                break;
            }
            nodes.entry(d.key()).or_insert_with(|| (d.clone(), dir_meta(), None));
            cur = d.parent();
        }
        nodes.insert(p.key(), (p, meta, target));
    }

    pub fn dir(&mut self, path: &str) -> &mut Self {
        self.insert(wp(path), dir_meta(), None);
        self
    }

    pub fn file(&mut self, path: &str, size: u64, modified: i64) -> &mut Self {
        self.insert(wp(path), EntryMeta { is_dir: false, is_reparse: false, size, modified: Some(modified) }, None);
        self
    }

    /// A junction pointing at `target` (only used for reporting).
    pub fn junction(&mut self, path: &str) -> &mut Self {
        self.junction_to(path, "C:\\elsewhere");
        self
    }

    pub fn junction_to(&self, path: &str, target: &str) {
        self.insert(
            wp(path),
            EntryMeta { is_dir: true, is_reparse: true, size: 0, modified: Some(0) },
            Some(target.into()),
        );
    }

    /// Replace an existing directory with a junction at runtime (race tests).
    /// Its previous children stay in the map but become unreachable.
    pub fn replace_with_junction(&self, path: &str, target: &str) {
        self.junction_to(path, target);
    }

    /// Add a directory entry with an exact on-disk name that Win32 path
    /// normalization would change (trailing dot or space).
    pub fn raw_entry(&mut self, parent: &str, name: &str, meta: EntryMeta) -> &mut Self {
        self.raw.lock().unwrap().entry(wp(parent).key()).or_default().push(DirEntry { name: name.into(), meta });
        self
    }

    pub fn deny(&mut self, path: &str) -> &mut Self {
        self.denied.lock().unwrap().insert(wp(path).key());
        self
    }

    /// If a strict ancestor of `path` is a reparse point, where it leads.
    fn redirected(&self, path: &WinPath) -> Option<String> {
        let nodes = self.nodes.lock().unwrap();
        let mut cur = path.parent();
        while let Some(d) = cur {
            if let Some((_, m, target)) = nodes.get(&d.key()) {
                if m.is_reparse {
                    let rest = &path.components()[d.components().len()..];
                    return Some(format!("{}\\{}", target.clone().unwrap_or_default(), rest.join("\\")));
                }
            }
            cur = d.parent();
        }
        None
    }

    fn run_hook(&self, hook: &Mutex<Option<Hook>>, path: &WinPath) {
        let taken = hook.lock().unwrap().take();
        if let Some(mut h) = taken {
            h(path, self);
            *hook.lock().unwrap() = Some(h);
        }
    }
}

impl FsRead for FakeFs {
    fn metadata(&self, path: &WinPath) -> Result<EntryMeta, FsError> {
        if path.is_root() {
            return Ok(EntryMeta { is_dir: true, is_reparse: false, size: 0, modified: None });
        }
        if let Some(to) = self.redirected(path) {
            return Err(FsError::Redirected(to));
        }
        self.nodes.lock().unwrap().get(&path.key()).map(|(_, m, _)| m.clone()).ok_or(FsError::NotFound)
    }

    fn launch_target_exists(&self, path: &WinPath) -> bool {
        // Follows junctions: the node exists, whatever its ancestors are.
        matches!(self.nodes.lock().unwrap().get(&path.key()), Some((_, m, _)) if !m.is_dir)
    }

    fn read_dir(&self, path: &WinPath) -> Result<Vec<DirEntry>, FsError> {
        self.run_hook(&self.on_read_dir, path);
        if self.denied.lock().unwrap().contains(&path.key()) {
            return Err(FsError::AccessDenied);
        }
        let meta = self.metadata(path)?;
        if !meta.is_dir {
            return Err(FsError::Other("not a directory".into()));
        }
        if meta.is_reparse {
            return Err(FsError::Other("refusing to list through a reparse point".into()));
        }
        let mut list: Vec<DirEntry> = self
            .nodes
            .lock()
            .unwrap()
            .values()
            .filter(|(p, _, _)| p.parent().as_ref() == Some(path))
            .map(|(p, m, _)| DirEntry { name: p.file_name().unwrap().to_string(), meta: m.clone() })
            .collect();
        list.extend(self.raw.lock().unwrap().get(&path.key()).cloned().unwrap_or_default());
        self.run_hook(&self.after_read_dir, path);
        Ok(list)
    }
}

type RegKey = (RegRoot, RegView, String);

#[derive(Default)]
pub struct FakeReg {
    keys: BTreeMap<RegKey, (String, BTreeMap<String, RegValue>)>,
    denied: BTreeSet<RegKey>,
}

fn rk(root: RegRoot, view: RegView, path: &str) -> RegKey {
    (root, view, fold(path))
}

impl FakeReg {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn key(&mut self, root: RegRoot, view: RegView, path: &str, values: &[(&str, RegValue)]) -> &mut Self {
        let mut parts: Vec<&str> = path.split('\\').collect();
        while parts.len() > 1 {
            parts.pop();
            let parent = parts.join("\\");
            self.keys.entry(rk(root, view, &parent)).or_insert_with(|| (parent.clone(), BTreeMap::new()));
        }
        let entry = self.keys.entry(rk(root, view, path)).or_insert_with(|| (path.to_string(), BTreeMap::new()));
        for (n, v) in values {
            entry.1.insert(n.to_string(), v.clone());
        }
        self
    }

    pub fn deny(&mut self, root: RegRoot, view: RegView, path: &str) -> &mut Self {
        self.denied.insert(rk(root, view, path));
        self
    }
}

impl RegRead for FakeReg {
    fn subkeys(&self, root: RegRoot, view: RegView, path: &str) -> Result<Vec<String>, RegError> {
        let k = rk(root, view, path);
        if self.denied.contains(&k) {
            return Err(RegError::AccessDenied);
        }
        if !self.keys.contains_key(&k) {
            return Err(RegError::NotFound);
        }
        let prefix = format!("{}\\", fold(path));
        Ok(self
            .keys
            .iter()
            .filter(|((r, v, p), _)| {
                *r == root && *v == view && p.starts_with(&prefix) && !p[prefix.len()..].contains('\\')
            })
            .map(|(_, (orig, _))| orig.rsplit('\\').next().unwrap().to_string())
            .collect())
    }

    fn values(&self, root: RegRoot, view: RegView, path: &str) -> Result<BTreeMap<String, RegValue>, RegError> {
        let k = rk(root, view, path);
        if self.denied.contains(&k) {
            return Err(RegError::AccessDenied);
        }
        self.keys.get(&k).map(|(_, v)| v.clone()).ok_or(RegError::NotFound)
    }
}

/// A typical Known Folders layout for user `alice` on drive C:.
pub fn known_folders() -> KnownFolders {
    let o = |s: &str| Some(wp(s));
    KnownFolders {
        windows: o("C:\\Windows"),
        system32: o("C:\\Windows\\System32"),
        syswow64: o("C:\\Windows\\SysWOW64"),
        program_files: o("C:\\Program Files"),
        program_files_x86: o("C:\\Program Files (x86)"),
        program_files_common: o("C:\\Program Files\\Common Files"),
        program_files_common_x86: o("C:\\Program Files (x86)\\Common Files"),
        program_data: o("C:\\ProgramData"),
        users: o("C:\\Users"),
        user_profile: o("C:\\Users\\alice"),
        public: o("C:\\Users\\Public"),
        roaming_app_data: o("C:\\Users\\alice\\AppData\\Roaming"),
        local_app_data: o("C:\\Users\\alice\\AppData\\Local"),
        local_programs: o("C:\\Users\\alice\\AppData\\Local\\Programs"),
        start_menu: o("C:\\Users\\alice\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu"),
        common_start_menu: o("C:\\ProgramData\\Microsoft\\Windows\\Start Menu"),
        start_menu_programs: o("C:\\Users\\alice\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs"),
        common_start_menu_programs: o("C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs"),
        desktop: o("C:\\Users\\alice\\Desktop"),
        common_desktop: o("C:\\Users\\Public\\Desktop"),
        // Redirected folder on another volume.
        documents: o("D:\\Users\\alice\\Documents"),
        pictures: o("C:\\Users\\alice\\Pictures"),
        music: o("C:\\Users\\alice\\Music"),
        videos: o("C:\\Users\\alice\\Videos"),
        downloads: o("C:\\Users\\alice\\Downloads"),
        one_drive: o("C:\\Users\\alice\\OneDrive"),
        temp: o("C:\\Users\\alice\\AppData\\Local\\Temp"),
        sweep_dirs: vec![wp("C:\\Program Files\\Sweep")],
        unresolved: vec![],
    }
}
