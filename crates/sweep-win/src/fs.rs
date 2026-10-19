//! Read-only filesystem access that never follows reparse points, including
//! reparse points in *ancestors* of the requested path.
//!
//! Every query opens a handle with `FILE_FLAG_OPEN_REPARSE_POINT` (so the
//! final component is never followed) and compares
//! `GetFinalPathNameByHandleW` with the requested path. If they differ, an
//! ancestor is a junction, symlink or mount point and the path is reported
//! as `FsError::Redirected` instead of being examined. Directory listings
//! are read from that same verified handle
//! (`GetFileInformationByHandleEx(FileFullDirectoryInfo)`), so a directory
//! swapped for a junction between the check and the listing cannot redirect
//! the listing.

use sweep_core::api::{DirEntry, EntryMeta, FsError, FsRead};
use sweep_core::winpath::WinPath;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::*;

const FILE_ATTRIBUTE_DIRECTORY_BIT: u32 = 0x10;
const FILE_ATTRIBUTE_REPARSE_POINT_BIT: u32 = 0x400;
const ACCESS_READ_ATTRIBUTES: u32 = 0x80;
const ACCESS_LIST_DIRECTORY: u32 = 0x1;
const ACCESS_SYNCHRONIZE: u32 = 0x0010_0000;
const ERROR_NO_MORE_FILES: u32 = 18;

pub struct WinFs;

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn win32_code(e: &windows::core::Error) -> u32 {
    (e.code().0 as u32) & 0xFFFF
}

fn map_err(e: &windows::core::Error) -> FsError {
    match win32_code(e) {
        2 | 3 => FsError::NotFound,
        5 => FsError::AccessDenied,
        _ => FsError::Other(e.message().to_string()),
    }
}

fn filetime_to_unix(ft: i64) -> Option<i64> {
    (ft > 0).then(|| ft / 10_000_000 - 11_644_473_600)
}

fn open(path: &WinPath, list: bool) -> Result<Handle, windows::core::Error> {
    let w = crate::wide::wide(&crate::wide::extended(path));
    let access = ACCESS_READ_ATTRIBUTES | ACCESS_SYNCHRONIZE | if list { ACCESS_LIST_DIRECTORY } else { 0 };
    unsafe {
        CreateFileW(
            PCWSTR(w.as_ptr()),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            HANDLE::default(),
        )
        .map(Handle)
    }
}

fn final_path(h: &Handle) -> Result<String, FsError> {
    let mut buf = vec![0u16; 1024];
    loop {
        let n = unsafe { GetFinalPathNameByHandleW(h.0, &mut buf, GETFINALPATHNAMEBYHANDLE_FLAGS(0)) } as usize;
        if n == 0 {
            return Err(FsError::Other("GetFinalPathNameByHandle failed".into()));
        }
        if n < buf.len() {
            return Ok(String::from_utf16_lossy(&buf[..n]));
        }
        buf.resize(n + 1, 0);
    }
}

/// The opened object must be exactly the requested path.
fn verify(path: &WinPath, h: &Handle) -> Result<(), FsError> {
    let f = final_path(h)?;
    match WinPath::from_api(&f) {
        Ok(p) if p == *path => Ok(()),
        _ => Err(FsError::Redirected(f.trim_start_matches("\\\\?\\").to_string())),
    }
}

fn info(h: &Handle) -> Result<EntryMeta, FsError> {
    let mut i = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(h.0, &mut i) }.map_err(|e| map_err(&e))?;
    let wt = ((i.ftLastWriteTime.dwHighDateTime as i64) << 32) | i.ftLastWriteTime.dwLowDateTime as i64;
    Ok(EntryMeta {
        is_dir: i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY_BIT != 0,
        is_reparse: i.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT_BIT != 0,
        size: ((i.nFileSizeHigh as u64) << 32) | i.nFileSizeLow as u64,
        modified: filetime_to_unix(wt),
    })
}

/// When a path is not found, an ancestor may be a junction whose target is
/// missing; that is a redirect, not an absence.
fn not_found_or_redirected(path: &WinPath) -> FsError {
    let mut cur = path.parent();
    while let Some(d) = cur {
        if d.is_root() {
            break;
        }
        if let Ok(m) = std::fs::symlink_metadata(crate::wide::extended(&d)) {
            use std::os::windows::fs::MetadataExt;
            if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT_BIT != 0 {
                return FsError::Redirected(format!("through junction/symlink at {d} (target unavailable)"));
            }
        }
        cur = d.parent();
    }
    FsError::NotFound
}

fn open_verified(path: &WinPath, list: bool) -> Result<Handle, FsError> {
    let h = open(path, list).map_err(|e| match map_err(&e) {
        FsError::NotFound => not_found_or_redirected(path),
        other => other,
    })?;
    verify(path, &h)?;
    Ok(h)
}

fn read_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn read_i64(b: &[u8], at: usize) -> i64 {
    i64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

impl FsRead for WinFs {
    fn launch_target_exists(&self, path: &WinPath) -> bool {
        // Follows links, like ShellExecuteEx will.
        std::fs::metadata(crate::wide::extended(path)).map(|m| m.is_file()).unwrap_or(false)
    }

    fn metadata(&self, path: &WinPath) -> Result<EntryMeta, FsError> {
        let h = open_verified(path, false)?;
        info(&h)
    }

    fn read_dir(&self, path: &WinPath) -> Result<Vec<DirEntry>, FsError> {
        let h = open_verified(path, true)?;
        let m = info(&h)?;
        if m.is_reparse {
            return Err(FsError::Other("refusing to list through a reparse point".into()));
        }
        if !m.is_dir {
            return Err(FsError::Other("not a directory".into()));
        }
        // FILE_FULL_DIR_INFO layout: NextEntryOffset@0, LastWriteTime@24,
        // EndOfFile@40, FileAttributes@56, FileNameLength@60, FileName@68.
        let mut buf = vec![0u64; 8192];
        let mut out = Vec::new();
        let mut class = FileFullDirectoryRestartInfo;
        loop {
            let r =
                unsafe { GetFileInformationByHandleEx(h.0, class, buf.as_mut_ptr() as *mut _, (buf.len() * 8) as u32) };
            class = FileFullDirectoryInfo;
            if let Err(e) = r {
                if win32_code(&e) == ERROR_NO_MORE_FILES {
                    break;
                }
                return Err(map_err(&e));
            }
            let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, buf.len() * 8) };
            let mut off = 0usize;
            loop {
                let next = read_u32(bytes, off) as usize;
                let attrs = read_u32(bytes, off + 56);
                let name_len = read_u32(bytes, off + 60) as usize;
                let name_u16: Vec<u16> = bytes[off + 68..off + 68 + name_len]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let name = String::from_utf16_lossy(&name_u16);
                if name != "." && name != ".." {
                    out.push(DirEntry {
                        name,
                        meta: EntryMeta {
                            is_dir: attrs & FILE_ATTRIBUTE_DIRECTORY_BIT != 0,
                            is_reparse: attrs & FILE_ATTRIBUTE_REPARSE_POINT_BIT != 0,
                            size: read_i64(bytes, off + 40).max(0) as u64,
                            modified: filetime_to_unix(read_i64(bytes, off + 24)),
                        },
                    });
                }
                if next == 0 {
                    break;
                }
                off += next;
            }
        }
        Ok(out)
    }
}
