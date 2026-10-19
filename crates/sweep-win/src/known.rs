//! Known Folders via SHGetKnownFolderPath for the current user.

use sweep_core::api::KnownFolders;
use sweep_core::winpath::WinPath;
use windows::core::GUID;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::*;

/// Resolve one Known Folder. `Ok(None)` means not defined on this system;
/// `Err` means it exists but cannot be used (e.g. redirected to a network
/// share), which the scanner reports as a coverage gap.
fn resolve(id: &GUID) -> Result<Option<WinPath>, String> {
    unsafe {
        let p = match SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG(0), None) {
            Ok(p) => p,
            // E_INVALIDARG / ERROR_FILE_NOT_FOUND / ERROR_PATH_NOT_FOUND: not defined.
            Err(e) if matches!(e.code().0 as u32, 0x8007_0057 | 0x8007_0002 | 0x8007_0003) => return Ok(None),
            Err(e) => return Err(format!("SHGetKnownFolderPath failed: {e}")),
        };
        let s = p.to_string();
        CoTaskMemFree(Some(p.0 as _));
        let s = s.map_err(|e| e.to_string())?;
        WinPath::from_api(&s).map(Some).map_err(|e| format!("{s}: {e}"))
    }
}
pub fn known_folders() -> KnownFolders {
    let mut unresolved = Vec::new();
    let mut get = |name: &str, id: &GUID| match resolve(id) {
        Ok(p) => p,
        Err(why) => {
            unresolved.push((name.to_string(), why));
            None
        }
    };
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_string_lossy().into_owned()))
        .and_then(|s| WinPath::from_api(&s).ok());
    let mut kf = KnownFolders {
        windows: get("Windows", &FOLDERID_Windows),
        system32: get("System", &FOLDERID_System),
        syswow64: get("SystemX86", &FOLDERID_SystemX86),
        program_files: get("ProgramFilesX64", &FOLDERID_ProgramFilesX64)
            .or_else(|| get("ProgramFiles", &FOLDERID_ProgramFiles)),
        program_files_x86: get("ProgramFilesX86", &FOLDERID_ProgramFilesX86),
        program_files_common: get("ProgramFilesCommonX64", &FOLDERID_ProgramFilesCommonX64)
            .or_else(|| get("ProgramFilesCommon", &FOLDERID_ProgramFilesCommon)),
        program_files_common_x86: get("ProgramFilesCommonX86", &FOLDERID_ProgramFilesCommonX86),
        program_data: get("ProgramData", &FOLDERID_ProgramData),
        users: get("UserProfiles", &FOLDERID_UserProfiles),
        user_profile: get("Profile", &FOLDERID_Profile),
        public: get("Public", &FOLDERID_Public),
        roaming_app_data: get("RoamingAppData", &FOLDERID_RoamingAppData),
        local_app_data: get("LocalAppData", &FOLDERID_LocalAppData),
        local_programs: get("UserProgramFiles", &FOLDERID_UserProgramFiles),
        start_menu: get("StartMenu", &FOLDERID_StartMenu),
        common_start_menu: get("CommonStartMenu", &FOLDERID_CommonStartMenu),
        start_menu_programs: get("Programs", &FOLDERID_Programs),
        common_start_menu_programs: get("CommonPrograms", &FOLDERID_CommonPrograms),
        desktop: get("Desktop", &FOLDERID_Desktop),
        common_desktop: get("PublicDesktop", &FOLDERID_PublicDesktop),
        documents: get("Documents", &FOLDERID_Documents),
        pictures: get("Pictures", &FOLDERID_Pictures),
        music: get("Music", &FOLDERID_Music),
        videos: get("Videos", &FOLDERID_Videos),
        downloads: get("Downloads", &FOLDERID_Downloads),
        one_drive: get("SkyDrive", &FOLDERID_SkyDrive),
        // %TEMP% is often an 8.3 short path (e.g. C:\Users\RUNNER~1\...);
        // resolve it to the long final path so comparisons work.
        temp: std::fs::canonicalize(std::env::temp_dir())
            .ok()
            .and_then(|p| WinPath::from_api(&p.to_string_lossy()).ok()),
        sweep_dirs: exe_dir.into_iter().collect(),
        unresolved: Vec::new(),
    };
    kf.unresolved = unresolved;
    kf
}
