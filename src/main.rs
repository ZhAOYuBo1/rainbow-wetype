mod paths;
mod signatures;
mod win;

use anyhow::{Context, anyhow};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use paths::{
    RendererInfo, backup_path, candidates, find_service, renderer_info, resolve_dir, service_image,
};
use signatures::{ScanResult, SignatureFile, choose_unique, decode_color, load_signatures, scan};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceOut {
    name: String,
    status: String,
    image_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DetectOut {
    ok: bool,
    action: &'static str,
    is_admin: bool,
    selected: Option<String>,
    candidates: Vec<String>,
    renderer: Option<RendererInfo>,
    service: Option<ServiceOut>,
    backup: Option<String>,
    backup_exists: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanOut {
    ok: bool,
    action: &'static str,
    selected: String,
    renderer: RendererInfo,
    matches: Vec<ScanResult>,
    signature: Option<String>,
    start: Option<usize>,
    current_color: Option<String>,
    supported: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplyOut {
    ok: bool,
    action: &'static str,
    color: String,
    selected: String,
    renderer: RendererInfo,
    signature: String,
    backup: String,
    service: Option<ServiceOut>,
    verified_color: Option<String>,
    matches_expected: bool,
    rolled_back: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestoreOut {
    ok: bool,
    action: &'static str,
    selected: String,
    renderer: RendererInfo,
    backup: String,
    service: Option<ServiceOut>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VerifyOut {
    ok: bool,
    action: &'static str,
    selected: String,
    renderer: RendererInfo,
    signature: Option<String>,
    current_color: Option<String>,
    expected_color: Option<String>,
    matches_expected: bool,
    error: Option<String>,
}

fn out<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn err_json(action: &str, error: impl ToString) -> ! {
    let value = serde_json::json!({
        "ok": false,
        "action": action,
        "error": error.to_string()
    });
    println!("{value}");
    std::process::exit(1);
}

fn default_signatures_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for candidate in [
                dir.join("signatures.json"),
                dir.parent()
                    .map(|p| p.join("signatures.json"))
                    .unwrap_or_default(),
            ] {
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let candidate = cwd.join("signatures.json");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("signatures.json")
}

fn signatures_path(arg: Option<&str>) -> PathBuf {
    arg.map(PathBuf::from)
        .unwrap_or_else(default_signatures_path)
}

fn service_out(dir: &str) -> Option<ServiceOut> {
    let name = find_service(dir)?;
    let state = win::service_state(&name).ok();
    Some(ServiceOut {
        status: state
            .map(win::service_state_name)
            .unwrap_or("Unknown")
            .to_string(),
        image_path: service_image(&name),
        name,
    })
}

fn dir_renderer(dir: &str) -> PathBuf {
    Path::new(dir).join("wetype_renderer.exe")
}

fn detect(explicit_dir: Option<&str>) -> anyhow::Result<DetectOut> {
    let all = candidates();
    let selected = if let Some(d) = explicit_dir {
        Some(resolve_dir(Some(d))?)
    } else if all.is_empty() {
        None
    } else {
        Some(resolve_dir(None)?)
    };
    let renderer = selected.as_ref().map(|d| dir_renderer(d));
    let info = match &renderer {
        Some(r) if r.is_file() => Some(renderer_info(r)?),
        _ => None,
    };
    let backup = renderer
        .as_ref()
        .filter(|r| r.is_file())
        .map(|r| backup_path(r));
    Ok(DetectOut {
        ok: selected.is_some(),
        action: "detect",
        is_admin: win::is_admin(),
        selected,
        candidates: all,
        renderer: info,
        service: None,
        backup: backup.as_ref().map(|p| p.to_string_lossy().to_string()),
        backup_exists: backup.as_ref().map(|p| p.is_file()).unwrap_or(false),
        error: None,
    })
    .map(|mut d| {
        d.service = d.selected.as_deref().and_then(service_out);
        d
    })
}

fn do_scan(dir: &str, sig: &SignatureFile) -> anyhow::Result<(ScanOut, Vec<u8>)> {
    let renderer = dir_renderer(dir);
    let data = fs::read(&renderer).with_context(|| format!("read {}", renderer.display()))?;
    let matches = scan(&data, sig)?;
    let chosen = choose_unique(&matches);
    let signature = chosen.map(|m| m.name.clone());
    let start = chosen.and_then(|m| m.start);
    let current_color = start.and_then(|s| decode_color(&data, s, &sig.offsets).ok());
    let has_chosen = chosen.is_some();
    let err = if has_chosen {
        None
    } else {
        Some("No unique candidate-brush signature; read references/reverse-engineering.md before changing anything.".to_string())
    };
    let out = ScanOut {
        ok: has_chosen,
        action: "scan",
        selected: dir.to_string(),
        renderer: renderer_info(&renderer)?,
        matches,
        signature,
        start,
        current_color,
        supported: has_chosen,
        error: err,
    };
    Ok((out, data))
}

fn original_colors(data: &[u8], start: usize, sig: &SignatureFile) -> bool {
    let o = &sig.offsets;
    let Some(r) = start.checked_add(o.r) else {
        return false;
    };
    if r + 4 > data.len() {
        return false;
    }
    data[r] == 0
        && data[r + 1] == 0
        && data[r + 2] == 0
        && data[r + 3] == 0
        && data.get(start + o.g) == Some(&0xB1)
        && data.get(start + o.g_dark) == Some(&0xA8)
        && data.get(start + o.b) == Some(&0x76)
        && data.get(start + o.b_dark) == Some(&0x6F)
}

fn write_retry(path: &Path, data: &[u8], dir: &Path) -> anyhow::Result<()> {
    let mut last = None;
    for _ in 0..15 {
        match fs::write(path, data) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                let _ = win::kill_installed(
                    dir,
                    &[
                        "wetype_renderer.exe",
                        "wetype_server.exe",
                        "wetype_update.exe",
                    ],
                );
                sleep(Duration::from_millis(600));
            }
        }
    }
    Err(anyhow!(
        "write failed after retries: {}",
        last.map(|e| e.to_string()).unwrap_or_default()
    ))
}

fn stop_for_write(dir: &str) -> (Option<ServiceOut>, Option<String>) {
    let mut warning = None;
    if let Some(name) = find_service(dir) {
        if let Err(e) = win::stop_service(&name) {
            warning = Some(format!("stop service: {e}"));
        }
    }
    let dir = Path::new(dir);
    for _ in 0..6 {
        let _ = win::kill_installed(
            dir,
            &[
                "wetype_renderer.exe",
                "wetype_server.exe",
                "wetype_update.exe",
            ],
        );
        sleep(Duration::from_millis(400));
    }
    sleep(Duration::from_millis(800));
    (service_out(dir.to_string_lossy().as_ref()), warning)
}

fn start_after_write(dir: &str) -> Option<String> {
    let Some(name) = find_service(dir) else {
        return None;
    };
    match win::start_service(&name) {
        Ok(()) => None,
        Err(e) => Some(format!("start service: {e}")),
    }
}

fn apply(color: &str, explicit_dir: Option<&str>, sig_path: &Path) -> anyhow::Result<ApplyOut> {
    if !win::is_admin() {
        return Err(anyhow!("administrator_required"));
    }
    let color = normalize_color(color)?;
    let dir = resolve_dir(explicit_dir)?;
    let renderer = dir_renderer(&dir);
    let sig = load_signatures(sig_path)?;
    let (scan_out, mut data) = do_scan(&dir, &sig)?;
    let chosen = scan_out
        .signature
        .as_ref()
        .and_then(|name| scan_out.matches.iter().find(|m| &m.name == name))
        .cloned()
        .ok_or_else(|| anyhow!("no unique signature"))?;
    let start = chosen.start.ok_or_else(|| anyhow!("no signature start"))?;
    let backup = backup_path(&renderer);
    let backup_exists = backup.is_file();
    if chosen.kind == "struct" && !original_colors(&data, start, &sig) && !backup_exists {
        return Err(anyhow!(
            "struct signature matched but colors are unknown and no backup exists"
        ));
    }
    if let Some(parent) = backup.parent() {
        fs::create_dir_all(parent)?;
    }
    if !backup_exists {
        fs::copy(&renderer, &backup)?;
    }

    let (_, stop_warning) = stop_for_write(&dir);
    let r = color_r(&color);
    let r_off = start + sig.offsets.r;
    if r_off + 4 > data.len() {
        return Err(anyhow!("R offset outside renderer"));
    }
    data[r_off..r_off + 4].copy_from_slice(&(r / 255.0).to_le_bytes());
    data[start + sig.offsets.g] = color_g(&color);
    data[start + sig.offsets.g_dark] = color_g(&color);
    data[start + sig.offsets.b] = color_b(&color);
    data[start + sig.offsets.b_dark] = color_b(&color);

    let write_result = write_retry(&renderer, &data, Path::new(&dir));

    if write_result.is_err() {
        if backup.is_file() {
            if let Ok(original) = fs::read(&backup) {
                let _ = write_retry(&renderer, &original, Path::new(&dir));
            }
        }
        let _ = start_after_write(&dir);
        return Err(write_result.unwrap_err());
    }
    let start_warning = start_after_write(&dir);
    let verify_data = fs::read(&renderer)?;
    let verified_color = chosen
        .start
        .and_then(|s| decode_color(&verify_data, s, &sig.offsets).ok());
    let matches_expected = verified_color.as_deref() == Some(color.as_str());
    let service = service_out(&dir);
    let mut error = None;
    if let Some(w) = stop_warning {
        error = Some(w);
    }
    if let Some(w) = start_warning {
        error = Some(match error {
            Some(prev) => format!("{prev}; {w}"),
            None => w,
        });
    }
    Ok(ApplyOut {
        ok: matches_expected,
        action: "apply",
        color,
        selected: dir,
        renderer: renderer_info(&renderer)?,
        signature: chosen.name,
        backup: backup.to_string_lossy().to_string(),
        service,
        verified_color,
        matches_expected,
        rolled_back: false,
        error,
    })
}

fn restore(explicit_dir: Option<&str>) -> anyhow::Result<RestoreOut> {
    if !win::is_admin() {
        return Err(anyhow!("administrator_required"));
    }
    let dir = resolve_dir(explicit_dir)?;
    let renderer = dir_renderer(&dir);
    let backup = backup_path(&renderer);
    if !backup.is_file() {
        return Err(anyhow!("original backup not found: {}", backup.display()));
    }
    let (_, stop_warning) = stop_for_write(&dir);
    let result = (|| {
        let data = fs::read(&backup)?;
        write_retry(&renderer, &data, Path::new(&dir))?;
        let _ = start_after_write(&dir);
        Ok::<(), anyhow::Error>(())
    })();
    if let Err(e) = result {
        let _ = start_after_write(&dir);
        return Err(e);
    }
    Ok(RestoreOut {
        ok: true,
        action: "restore",
        selected: dir.clone(),
        renderer: renderer_info(&renderer)?,
        backup: backup.to_string_lossy().to_string(),
        service: service_out(&dir),
        error: stop_warning,
    })
}

fn verify(
    explicit_dir: Option<&str>,
    expect: Option<&str>,
    sig_path: &Path,
) -> anyhow::Result<VerifyOut> {
    let dir = resolve_dir(explicit_dir)?;
    let sig = load_signatures(sig_path)?;
    let (scan_out, _data) = do_scan(&dir, &sig)?;
    let expected = expect.map(normalize_color).transpose()?;
    let matches_expected = match (&expected, &scan_out.current_color) {
        (Some(want), Some(got)) => want == got,
        (None, _) => true,
        _ => false,
    };
    Ok(VerifyOut {
        ok: scan_out.supported,
        action: "verify",
        selected: dir,
        renderer: scan_out.renderer,
        signature: scan_out.signature,
        current_color: scan_out.current_color,
        expected_color: expected,
        matches_expected,
        error: scan_out.error,
    })
}

fn normalize_color(input: &str) -> anyhow::Result<String> {
    let mut c = input.trim().to_string();
    if !c.starts_with('#') {
        c.insert(0, '#');
    }
    if c.len() != 7 || !c[1..].chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(anyhow!("color must be #RRGGBB"));
    }
    Ok(c.to_uppercase())
}

fn color_r(c: &str) -> f32 {
    u8::from_str_radix(&c[1..3], 16).unwrap() as f32
}
fn color_g(c: &str) -> u8 {
    u8::from_str_radix(&c[3..5], 16).unwrap()
}
fn color_b(c: &str) -> u8 {
    u8::from_str_radix(&c[5..7], 16).unwrap()
}

fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let action = args.first().map(String::as_str).unwrap_or("detect");
    if action == "--help" || action == "-h" {
        println!(
            "rainbow-wetype: detect|scan|apply|restore|verify [--dir PATH] [--color #RRGGBB] [--expect #RRGGBB] [--signatures FILE]"
        );
        return Ok(());
    }
    let mut dir: Option<String> = None;
    let mut color_arg: Option<String> = None;
    let mut expect_arg: Option<String> = None;
    let mut sig_arg: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--dir" if i + 1 < args.len() => {
                dir = Some(args[i + 1].clone());
                i += 2;
            }
            "--color" if i + 1 < args.len() => {
                color_arg = Some(args[i + 1].clone());
                i += 2;
            }
            "--expect" if i + 1 < args.len() => {
                expect_arg = Some(args[i + 1].clone());
                i += 2;
            }
            "--signatures" if i + 1 < args.len() => {
                sig_arg = Some(args[i + 1].clone());
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }
    let sig_path = signatures_path(sig_arg.as_deref());
    match action {
        "detect" => out(&detect(dir.as_deref())?),
        "scan" => {
            let selected = resolve_dir(dir.as_deref())?;
            let sig = load_signatures(&sig_path)?;
            let (scan_out, _) = do_scan(&selected, &sig)?;
            out(&scan_out)
        }
        "apply" => {
            let color = color_arg.ok_or_else(|| anyhow!("--color is required"))?;
            out(&apply(&color, dir.as_deref(), &sig_path)?)
        }
        "restore" => out(&restore(dir.as_deref())?),
        "verify" => out(&verify(dir.as_deref(), expect_arg.as_deref(), &sig_path)?),
        _ => Err(anyhow!("unknown action: {action}")),
    }
}

fn main() {
    if let Err(e) = run() {
        err_json(std::env::args().nth(1).as_deref().unwrap_or("unknown"), e);
    }
}
