use anyhow::{Context, anyhow};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use sysinfo::{ProcessesToUpdate, System};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererInfo {
    pub path: String,
    pub size: u64,
    pub version: String,
    pub sha256: String,
}

fn renderer_in(dir: &Path) -> Option<PathBuf> {
    let p = dir.join("wetype_renderer.exe");
    if p.is_file() { Some(p) } else { None }
}

fn add_dir_candidate(list: &mut BTreeSet<String>, dir: impl AsRef<Path>) {
    let dir = dir.as_ref();
    if !dir.exists() {
        return;
    }
    if renderer_in(dir).is_some() {
        list.insert(dir.to_string_lossy().to_string());
        return;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let sub = entry.path();
            if sub.is_dir() && renderer_in(&sub).is_some() {
                list.insert(sub.to_string_lossy().to_string());
            }
        }
    }
}

fn add_file_parent(list: &mut BTreeSet<String>, file: &str) {
    let file = file.trim().trim_matches('"');
    if file.is_empty() {
        return;
    }
    if let Some(parent) = Path::new(file).parent() {
        add_dir_candidate(list, parent);
    }
}

fn add_process_candidates(list: &mut BTreeSet<String>) {
    if let Ok(mut sys) = System::new_all() {
        sys.refresh_processes(ProcessesToUpdate::All, true);
        for process in sys.processes().values() {
            let Some(exe) = process.exe() else { continue };
            let name = exe
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if name == "wetype_renderer.exe"
                || name == "wetype_server.exe"
                || name == "wetype_service.exe"
            {
                if let Some(parent) = exe.parent() {
                    add_dir_candidate(list, parent);
                }
            }
        }
    }
}

fn add_registry_candidates(list: &mut BTreeSet<String>) {
    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        let root = RegKey::predef(hive);
        for uninstall_path in [
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ] {
            let Ok(uninstall) = root.open_subkey(uninstall_path) else {
                continue;
            };
            for name in uninstall.enum_keys().flatten() {
                let Ok(entry) = uninstall.open_subkey(&name) else {
                    continue;
                };
                let display: String = entry.get_value("DisplayName").unwrap_or_default();
                let lower = display.to_lowercase();
                if !(lower.contains("wetype")
                    || display.contains("微信输入法")
                    || lower.contains("wechat input"))
                {
                    continue;
                }
                if let Ok(location) = entry.get_value::<String, _>("InstallLocation") {
                    add_dir_candidate(list, location.trim().trim_matches('"'));
                }
                if let Ok(icon) = entry.get_value::<String, _>("DisplayIcon") {
                    add_file_parent(list, icon.split(',').next().unwrap_or(""));
                }
                if let Ok(uninstall_cmd) = entry.get_value::<String, _>("UninstallString") {
                    add_file_parent(list, uninstall_cmd.split_whitespace().next().unwrap_or(""));
                }
            }
        }
        if let Ok(key) = root.open_subkey(r"SOFTWARE\Tencent\WeType") {
            for name in key.enum_values().flatten().map(|(n, _)| n) {
                if let Ok(value) = key.get_value::<String, _>(&name) {
                    add_dir_candidate(list, value.trim().trim_matches('"'));
                }
            }
        }
    }
}

fn add_conventional_candidates(list: &mut BTreeSet<String>) {
    for base in [
        std::env::var("ProgramFiles").ok(),
        std::env::var("ProgramFiles(x86)").ok(),
        Some(r"C:\Program Files".to_string()),
        Some(r"C:\Program Files (x86)".to_string()),
    ]
    .into_iter()
    .flatten()
    {
        add_dir_candidate(list, Path::new(&base).join("Tencent").join("WeType"));
    }
}

pub fn candidates() -> Vec<String> {
    let mut set = BTreeSet::new();
    add_process_candidates(&mut set);
    add_registry_candidates(&mut set);
    add_conventional_candidates(&mut set);
    set.into_iter().collect()
}

pub fn version_of(renderer: &Path) -> String {
    renderer
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|v| v.to_str())
        .unwrap_or("unknown")
        .to_string()
}

pub fn renderer_info(renderer: &Path) -> anyhow::Result<RendererInfo> {
    let data = fs::read(renderer).with_context(|| format!("read {}", renderer.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = hex::encode(hasher.finalize());
    Ok(RendererInfo {
        path: renderer.to_string_lossy().to_string(),
        size: data.len() as u64,
        version: version_of(renderer),
        sha256: hash.to_uppercase(),
    })
}

pub fn resolve_dir(explicit: Option<&str>) -> anyhow::Result<String> {
    if let Some(input) = explicit {
        let input = input.trim().trim_matches('"');
        if input.is_empty() {
            return Err(anyhow!("empty --dir"));
        }
        let p = Path::new(input);
        if renderer_in(p).is_some() {
            return Ok(p.to_string_lossy().to_string());
        }
        if p.is_dir() {
            let mut children: Vec<PathBuf> = fs::read_dir(p)?
                .flatten()
                .map(|e| e.path())
                .filter(|x| x.is_dir() && renderer_in(x).is_some())
                .collect();
            children.sort();
            if let Some(preferred) = children
                .iter()
                .find(|x| version_of(&renderer_in(x).unwrap()) == "3.0.2.44")
            {
                return Ok(preferred.to_string_lossy().to_string());
            }
            if let Some(last) = children.last() {
                return Ok(last.to_string_lossy().to_string());
            }
        }
        return Err(anyhow!("wetype_renderer.exe not found under: {input}"));
    }

    let all = candidates();
    if all.is_empty() {
        return Err(anyhow!("WeType installation not found"));
    }
    if let Some(preferred) = all
        .iter()
        .find(|d| version_of(&Path::new(d).join("wetype_renderer.exe")) == "3.0.2.44")
    {
        return Ok(preferred.clone());
    }
    Ok(all[0].clone())
}

pub fn program_data() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}

pub fn backup_path(renderer: &Path) -> PathBuf {
    let version = version_of(renderer);
    program_data()
        .join("WeTypeColor")
        .join("backup")
        .join(version)
        .join("wetype_renderer.exe.orig")
}

pub fn find_service(install_dir: &str) -> Option<String> {
    let target = install_dir.trim_end_matches('\\').to_ascii_lowercase();
    let root = RegKey::predef(HKEY_LOCAL_MACHINE);
    let services = root
        .open_subkey(r"SYSTEM\CurrentControlSet\Services")
        .ok()?;
    let mut fallback = None;
    for name in services.enum_keys().flatten() {
        let Ok(key) = services.open_subkey(&name) else {
            continue;
        };
        let image: String = key.get_value("ImagePath").unwrap_or_default();
        let display: String = key.get_value("DisplayName").unwrap_or_default();
        let image_clean = image
            .trim()
            .trim_matches('"')
            .trim_start_matches(r"\??\")
            .to_ascii_lowercase();
        if !image_clean.is_empty() && image_clean.starts_with(&(target.clone() + "\\")) {
            return Some(name);
        }
        if fallback.is_none()
            && (name.to_ascii_lowercase().contains("wetype")
                || display.contains("微信输入法")
                || display.to_ascii_lowercase().contains("wetype"))
        {
            fallback = Some(name);
        }
    }
    fallback
}

pub fn service_image(name: &str) -> String {
    let root = RegKey::predef(HKEY_LOCAL_MACHINE);
    root.open_subkey(format!(r"SYSTEM\CurrentControlSet\Services\{name}"))
        .ok()
        .and_then(|key| key.get_value::<String, _>("ImagePath").ok())
        .unwrap_or_default()
}
