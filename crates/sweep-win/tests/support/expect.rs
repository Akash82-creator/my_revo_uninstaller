//! Selection of the registration a real-app test may uninstall.
//!
//! CI must never uninstall something other than what it just installed. A
//! registration is selected only if it matches the pinned version, scope,
//! installer type and (for MSI) ProductCode, *and* it did not exist before
//! the installers ran. If an installer failed while the runner already had a
//! matching product, selection fails instead of uninstalling that product.

use serde::{Deserialize, Serialize};
use sweep_core::inventory::Inventory;
use sweep_core::model::{InstalledProgram, RegView, Scope};

pub struct Expect {
    pub name_prefix: &'static str,
    /// Pinned version, compared numerically ("24.08" == "24.08.00.0").
    pub version: &'static str,
    pub scope: Scope,
    /// `Some(code)` for an MSI product; `None` for an EXE-based installer.
    pub msi_product_code: Option<&'static str>,
}

/// Identity of one registration, recorded before installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegId {
    pub scope: Scope,
    pub view: RegView,
    pub key: String,
    pub version: Option<String>,
}

pub fn reg_ids(inv: &Inventory) -> Vec<RegId> {
    inv.programs
        .iter()
        .flat_map(|p| {
            p.sources.iter().map(move |s| RegId {
                scope: s.scope,
                view: s.view,
                key: s.key_name.to_lowercase(),
                version: p.version.clone(),
            })
        })
        .collect()
}

fn version_parts(v: &str) -> Vec<u64> {
    let mut parts: Vec<u64> = v.trim().split('.').map(|p| p.trim().parse().unwrap_or(u64::MAX)).collect();
    while parts.len() > 1 && parts.last() == Some(&0) {
        parts.pop();
    }
    parts
}

pub fn same_version(a: &str, b: &str) -> bool {
    version_parts(a) == version_parts(b)
}

pub fn select<'a>(inv: &'a Inventory, e: &Expect, preinstall: &[RegId]) -> Result<&'a InstalledProgram, String> {
    let named: Vec<&InstalledProgram> =
        inv.programs.iter().filter(|p| p.display_name.starts_with(e.name_prefix)).collect();
    let describe = |ps: &[&InstalledProgram]| {
        ps.iter()
            .map(|p| {
                format!(
                    "{} v{:?} {:?} msi={} code={:?}",
                    p.display_name, p.version, p.scope, p.windows_installer, p.product_code
                )
            })
            .collect::<Vec<_>>()
    };
    let matching: Vec<&InstalledProgram> = named
        .iter()
        .copied()
        .filter(|p| p.windows_installer == e.msi_product_code.is_some())
        .filter(|p| p.scope == e.scope)
        .filter(|p| p.version.as_deref().is_some_and(|v| same_version(v, e.version)))
        .filter(|p| match e.msi_product_code {
            Some(code) => p.product_code.as_deref().is_some_and(|c| c.eq_ignore_ascii_case(code)),
            None => true,
        })
        .collect();
    let p = match matching.as_slice() {
        [one] => *one,
        [] => {
            return Err(format!(
                "no registration matches {} v{} {:?} msi={:?}; candidates by name: {:?}",
                e.name_prefix,
                e.version,
                e.scope,
                e.msi_product_code,
                describe(&named)
            ))
        }
        many => return Err(format!("ambiguous: {:?}", describe(many))),
    };
    for id in reg_ids_of(p) {
        if preinstall.iter().any(|pre| pre.scope == id.scope && pre.view == id.view && pre.key == id.key) {
            return Err(format!(
                "{} ({:?} {:?} {}) already existed before installation; refusing to treat it as the freshly installed product",
                p.display_name, id.scope, id.view, id.key
            ));
        }
    }
    Ok(p)
}

fn reg_ids_of(p: &InstalledProgram) -> Vec<RegId> {
    p.sources
        .iter()
        .map(|s| RegId { scope: s.scope, view: s.view, key: s.key_name.to_lowercase(), version: p.version.clone() })
        .collect()
}
