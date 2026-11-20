#![windows_subsystem = "windows"]

extern crate native_windows_gui as nwg;

use nwg::NativeUi;
use serde_json::Value;
use std::cell::RefCell;
use std::ops::Deref;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

static APP_ICON: &[u8] = include_bytes!("../assets/app.ico");

#[link(name = "user32")]
unsafe extern "system" {
    fn SendMessageW(hwnd: *mut std::ffi::c_void, msg: u32, wparam: usize, lparam: isize) -> isize;
}

#[derive(Default)]
struct App {
    window: nwg::Window,
    icon: nwg::Icon,
    dir_label: nwg::Label,
    dir_input: nwg::TextInput,
    color_label: nwg::Label,
    color_input: nwg::TextInput,
    browse_btn: nwg::Button,
    apply_btn: nwg::Button,
    restore_btn: nwg::Button,
    status: nwg::Label,
}

impl App {
    fn cli_path(&self) -> PathBuf {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
        let dir = exe.parent().unwrap_or(Path::new("."));
        let candidates = [
            dir.join("wetype-color.exe"),
            dir.join("rainbow-wetype.exe"),
            dir.parent()
                .map(|p| p.join("wetype-color.exe"))
                .unwrap_or_default(),
            dir.parent()
                .map(|p| p.join("rainbow-wetype.exe"))
                .unwrap_or_default(),
            std::env::current_dir()
                .map(|p| p.join("wetype-color.exe"))
                .unwrap_or_default(),
        ];
        candidates
            .into_iter()
            .find(|p| p.is_file())
            .unwrap_or_else(|| dir.join("wetype-color.exe"))
    }

    fn signature_path(&self) -> PathBuf {
        let cli = self.cli_path();
        let dir = cli.parent().unwrap_or(Path::new("."));
        for candidate in [
            dir.join("signatures.json"),
            dir.parent()
                .map(|p| p.join("signatures.json"))
                .unwrap_or_default(),
            std::env::current_dir()
                .map(|p| p.join("signatures.json"))
                .unwrap_or_default(),
        ] {
            if candidate.is_file() {
                return candidate;
            }
        }
        dir.join("signatures.json")
    }

    fn run_cli(&self, args: &[String]) -> Result<Value, String> {
        let mut command = Command::new(self.cli_path());
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        let output = command
            .args(args)
            .output()
            .map_err(|e| format!("无法启动 CLI：{e}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let value: Value = serde_json::from_str(stdout.trim())
            .map_err(|e| format!("CLI 输出无法解析：{e}\n{stdout}\n{stderr}"))?;
        if output.status.success() {
            Ok(value)
        } else {
            let err = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("CLI 返回失败")
                .to_string();
            Err(format!("{err}\n{stderr}"))
        }
    }

    fn set_status(&self, text: impl AsRef<str>) {
        self.status.set_text(text.as_ref());
    }

    fn refresh_detect(&self) {
        self.set_status("正在检测微信输入法...");
        let mut args = vec!["detect".to_string()];
        let dir = self.dir_input.text();
        if !dir.trim().is_empty() {
            args.push("--dir".to_string());
            args.push(dir);
        }
        match self.run_cli(&args) {
            Ok(value) => {
                if let Some(selected) = value.get("selected").and_then(Value::as_str) {
                    self.dir_input.set_text(selected);
                }
                let version = value
                    .pointer("/renderer/version")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let hash = value
                    .pointer("/renderer/sha256")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let service = value
                    .pointer("/service/status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let backup = value
                    .get("backupExists")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.set_status(format!(
                    "已检测到微信输入法 {version}\n服务：{service}    备份：{}\nSHA256：{hash}",
                    if backup { "有" } else { "无" }
                ));
            }
            Err(e) => self.set_status(format!("检测失败：{e}")),
        }
    }

    fn browse(&self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("选择微信输入法安装目录（包含 wetype_renderer.exe）")
            .pick_folder()
        {
            self.dir_input.set_text(&path.to_string_lossy());
            self.refresh_detect();
        }
    }

    fn apply(&self) {
        let color = self.color_input.text().trim().to_string();
        let dir = self.dir_input.text().trim().to_string();
        if color.is_empty() || dir.is_empty() {
            nwg::modal_info_message(&self.window, "提示", "请先填写颜色和安装目录。");
            return;
        }
        self.set_status(format!("正在应用 {color} ..."));
        let sigh = self.signature_path().to_string_lossy().to_string();
        let args = vec![
            "apply".to_string(),
            "--color".to_string(),
            color.clone(),
            "--dir".to_string(),
            dir,
            "--signatures".to_string(),
            sigh,
        ];
        match self.run_cli(&args) {
            Ok(value) => {
                let verified = value
                    .get("verifiedColor")
                    .and_then(Value::as_str)
                    .unwrap_or("?");
                let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(false);
                if ok {
                    self.set_status(format!(
                        "已应用：{verified}\n重启输入法后，回到输入框打字即可看到新候选色。"
                    ));
                } else {
                    let err = value
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("应用失败");
                    self.set_status(format!("应用失败：{err}"));
                }
            }
            Err(e) => {
                self.set_status(format!("应用失败：{e}"));
                nwg::modal_error_message(&self.window, "应用失败", &e);
            }
        }
    }

    fn restore(&self) {
        let params = nwg::MessageParams {
            title: "确认恢复",
            content: "恢复微信输入法原版候选框颜色？",
            buttons: nwg::MessageButtons::YesNo,
            icons: nwg::MessageIcons::Question,
        };
        if nwg::modal_message(&self.window, &params) != nwg::MessageChoice::Yes {
            return;
        }
        let dir = self.dir_input.text().trim().to_string();
        if dir.is_empty() {
            nwg::modal_info_message(&self.window, "提示", "请先确认安装目录。");
            return;
        }
        self.set_status("正在恢复原版...");
        let args = vec!["restore".to_string(), "--dir".to_string(), dir];
        match self.run_cli(&args) {
            Ok(_) => self.set_status("已恢复原版。"),
            Err(e) => {
                self.set_status(format!("恢复失败：{e}"));
                nwg::modal_error_message(&self.window, "恢复失败", &e);
            }
        }
    }
}

struct AppUi {
    inner: Rc<App>,
    default_handler: RefCell<Option<nwg::EventHandler>>,
}

impl nwg::NativeUi<AppUi> for App {
    fn build_ui(mut data: App) -> Result<AppUi, nwg::NwgError> {
        use nwg::Event as E;

        nwg::Icon::builder()
            .source_bin(Some(APP_ICON))
            .size(Some((32, 32)))
            .strict(true)
            .build(&mut data.icon)?;

        nwg::Window::builder()
            .icon(Some(&data.icon))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .size((720, 470))
            .position((360, 260))
            .title("微信输入法候选框改色工具 — Rust")
            .build(&mut data.window)?;

        if let Some(hwnd) = data.window.handle.hwnd() {
            unsafe {
                SendMessageW(
                    hwnd as *mut std::ffi::c_void,
                    0x80,
                    1,
                    data.icon.handle as isize,
                );
            }
        }

        nwg::Label::builder()
            .text("安装目录（自动检测，也可以手选）")
            .position((18, 16))
            .size((650, 22))
            .parent(&data.window)
            .build(&mut data.dir_label)?;

        nwg::TextInput::builder()
            .text("")
            .position((18, 42))
            .size((520, 28))
            .parent(&data.window)
            .build(&mut data.dir_input)?;

        nwg::Button::builder()
            .text("浏览...")
            .position((548, 41))
            .size((130, 30))
            .parent(&data.window)
            .build(&mut data.browse_btn)?;

        nwg::Label::builder()
            .text("候选框强调色（#RRGGBB）")
            .position((18, 90))
            .size((360, 22))
            .parent(&data.window)
            .build(&mut data.color_label)?;

        nwg::TextInput::builder()
            .text("#007AFF")
            .position((18, 116))
            .size((180, 30))
            .parent(&data.window)
            .build(&mut data.color_input)?;

        nwg::Button::builder()
            .text("应用并重启输入法")
            .position((18, 170))
            .size((190, 40))
            .parent(&data.window)
            .build(&mut data.apply_btn)?;

        nwg::Button::builder()
            .text("恢复原版")
            .position((220, 170))
            .size((140, 40))
            .parent(&data.window)
            .build(&mut data.restore_btn)?;

        nwg::Label::builder()
            .text("正在检测...")
            .position((18, 235))
            .size((660, 170))
            .parent(&data.window)
            .build(&mut data.status)?;

        let ui = AppUi {
            inner: Rc::new(data),
            default_handler: Default::default(),
        };

        let evt_ui = Rc::downgrade(&ui.inner);
        let handle_events = move |evt, _evt_data, handle| {
            if let Some(app) = evt_ui.upgrade() {
                match evt {
                    E::OnButtonClick => {
                        if &handle == &app.browse_btn {
                            app.browse();
                        } else if &handle == &app.apply_btn {
                            app.apply();
                        } else if &handle == &app.restore_btn {
                            app.restore();
                        }
                    }
                    E::OnWindowClose => {
                        if &handle == &app.window {
                            nwg::stop_thread_dispatch();
                        }
                    }
                    _ => {}
                }
            }
        };
        *ui.default_handler.borrow_mut() = Some(nwg::full_bind_event_handler(
            &ui.window.handle,
            handle_events,
        ));

        Ok(ui)
    }
}

impl Deref for AppUi {
    type Target = App;
    fn deref(&self) -> &App {
        &self.inner
    }
}

impl Drop for AppUi {
    fn drop(&mut self) {
        if let Some(handler) = self.default_handler.borrow().as_ref() {
            nwg::unbind_event_handler(handler);
        }
    }
}

fn main() {
    nwg::init().expect("Failed to init Native Windows GUI");
    let _ = nwg::Font::set_global_family("Microsoft YaHei UI");
    let ui = App::build_ui(App::default()).expect("Failed to build UI");
    ui.refresh_detect();
    nwg::dispatch_thread_events();
}
