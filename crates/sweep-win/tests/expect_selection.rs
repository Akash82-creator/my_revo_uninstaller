//! Regression tests for real-app selection (runs on any platform).
//! Scenario from review: an installer fails while the runner already has a
//! matching product; CI must not uninstall that product and report success.

#[path = "support/expect.rs"]
mod expect;

use expect::{reg_ids, same_version, select, Expect};
use sweep_core::inventory::{build_inventory, Inventory};
use sweep_core::model::{RegValue, RegView, Scope, UninstallEntry};

const CODE_2408: &str = "{23170F69-40C1-2702-2408-000001000000}";
const CODE_2407: &str = "{23170F69-40C1-2702-2407-000001000000}";

fn entry(view: RegView, key: &str, name: &str, version: &str, msi: bool) -> UninstallEntry {
    let mut values = vec![
        ("DisplayName".to_string(), RegValue::Sz(name.into())),
        ("DisplayVersion".to_string(), RegValue::Sz(version.into())),
        ("UninstallString".to_string(), RegValue::Sz(format!("\"C:\\Program Files\\{key}\\u.exe\""))),
    ];
    if msi {
        values.push(("WindowsInstaller".into(), RegValue::Dword(1)));
    }
    UninstallEntry { scope: Scope::Machine, view, key_name: key.into(), values: values.into_iter().collect() }
}

fn inv(entries: Vec<UninstallEntry>) -> Inventory {
    build_inventory(entries, vec![])
}

fn seven_zip_msi() -> Expect {
    Expect { name_prefix: "7-Zip", version: "24.08", scope: Scope::Machine, msi_product_code: Some(CODE_2408) }
}

fn msi_2408() -> UninstallEntry {
    entry(RegView::Reg64, CODE_2408, "7-Zip 24.08 (x64 edition)", "24.08.00.0", true)
}

#[test]
fn freshly_installed_msi_is_selected() {
    let before = inv(vec![entry(RegView::Reg64, "7-Zip", "7-Zip 24.08 (x64)", "24.08", false)]);
    let after = inv(vec![entry(RegView::Reg64, "7-Zip", "7-Zip 24.08 (x64)", "24.08", false), msi_2408()]);
    let p = select(&after, &seven_zip_msi(), &reg_ids(&before)).unwrap();
    assert_eq!(p.product_code.as_deref(), Some(CODE_2408));
}

#[test]
fn failed_install_with_preexisting_matching_product_is_rejected() {
    // The pinned MSI was already present; the install "ran" but changed nothing.
    let before = inv(vec![msi_2408()]);
    let after = inv(vec![msi_2408()]);
    let err = select(&after, &seven_zip_msi(), &reg_ids(&before)).unwrap_err();
    assert!(err.contains("already existed before installation"), "{err}");
}

#[test]
fn failed_install_with_preexisting_other_version_is_rejected() {
    let old = entry(RegView::Reg64, CODE_2407, "7-Zip 24.07 (x64 edition)", "24.07.00.0", true);
    let before = inv(vec![old.clone()]);
    let after = inv(vec![old]);
    let err = select(&after, &seven_zip_msi(), &reg_ids(&before)).unwrap_err();
    assert!(err.contains("no registration matches"), "{err}");
}

#[test]
fn exe_build_of_same_version_is_not_taken_for_msi() {
    let exe = entry(RegView::Reg64, "7-Zip", "7-Zip 24.08 (x64)", "24.08", false);
    let err = select(&inv(vec![exe]), &seven_zip_msi(), &[]).unwrap_err();
    assert!(err.contains("no registration matches"), "{err}");
}

#[test]
fn wrong_product_code_or_scope_is_rejected() {
    let wrong_code = entry(RegView::Reg64, CODE_2407, "7-Zip 24.08 (x64 edition)", "24.08.00.0", true);
    assert!(select(&inv(vec![wrong_code]), &seven_zip_msi(), &[]).is_err());
    let mut per_user = msi_2408();
    per_user.scope = Scope::User;
    per_user.view = RegView::Shared;
    assert!(select(&inv(vec![per_user]), &seven_zip_msi(), &[]).is_err());
}

#[test]
fn exe_installer_requires_pinned_version_and_fresh_registration() {
    let e = Expect { name_prefix: "Notepad++", version: "8.7.1", scope: Scope::Machine, msi_product_code: None };
    let npp = |v: &str| entry(RegView::Reg64, "Notepad++", "Notepad++ (64-bit x64)", v, false);
    assert!(select(&inv(vec![npp("8.7.1")]), &e, &[]).is_ok());
    assert!(select(&inv(vec![npp("8.6.9")]), &e, &[]).is_err(), "older version present, install failed");
    let before = inv(vec![npp("8.7.1")]);
    assert!(select(&inv(vec![npp("8.7.1")]), &e, &reg_ids(&before)).is_err(), "pre-existing same version");
}

#[test]
fn version_comparison_is_numeric() {
    assert!(same_version("24.08.00.0", "24.08"));
    assert!(same_version("6.3.3", "6.3.3.0"));
    assert!(!same_version("24.08", "24.8.1"));
    assert!(!same_version("8.7.10", "8.7.1"));
}
