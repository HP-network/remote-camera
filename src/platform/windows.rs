use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, Sleep, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_MOVE_NOCOALESCE, MOUSEINPUT, VK_F8, VK_F9,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EnumWindows, GetClientRect, GetForegroundWindow,
    GetSystemMetrics, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    PeekMessageW, SetCursorPos, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    HC_ACTION, HHOOK, MSG, MSLLHOOKSTRUCT, PM_REMOVE, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_REMOTESESSION, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WH_MOUSE_LL, WM_MOUSEMOVE, WM_QUIT,
};

use crate::config::Config;
use crate::engine::{CameraEngine, EngineState, TargetWindow};
use crate::platform::RunOptions;

static RUNTIME: OnceLock<usize> = OnceLock::new();
const INJECTED_TAG: usize = 0x5243_414D_4552;

struct Runtime {
    config: Config,
    options: RunOptions,
    engine: CameraEngine,
    user_enabled: bool,
    rdp_session: bool,
    last_probe: Instant,
    last_config_check: Instant,
    config_modified: Option<std::time::SystemTime>,
    target: Option<TargetWindow>,
}

struct HookGuard(HHOOK);

impl Drop for HookGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                UnhookWindowsHookEx(self.0);
            }
        }
    }
}

pub fn run(config: Config, options: RunOptions) {
    let now = Instant::now();
    let rdp_session = detect_rdp_session();
    let enabled_on_start = config.enabled_on_start;
    if config.session_mode == "rdp" && !rdp_session && !options.allow_local {
        println!("未检测到 RDP 会话。使用 --no-rdp 或配置 session_mode=any 可使用其他远控软件。");
    }

    let mut engine = CameraEngine::new(config.clone());
    engine.set_enabled(
        config.enabled_on_start
            && (config.session_mode != "rdp" || rdp_session || options.allow_local),
    );
    let runtime = Box::new(Runtime {
        config,
        options,
        engine,
        user_enabled: enabled_on_start,
        rdp_session,
        last_probe: now - Duration::from_secs(1),
        last_config_check: now,
        config_modified: None,
        target: None,
    });
    let runtime = Box::leak(runtime) as *mut Runtime;
    let _ = RUNTIME.set(runtime as usize);

    let hook =
        unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), GetModuleHandleW(null()), 0) };
    if hook.is_null() {
        eprintln!("无法安装 Windows 低级鼠标钩子，请在交互式 Windows 桌面运行。");
        return;
    }
    let _hook_guard = HookGuard(hook);

    println!(
        "Remote Camera {} 已启动。F8 切换，F9 退出。",
        env!("CARGO_PKG_VERSION")
    );
    println!(
        "backend=windows hook=WH_MOUSE_LL mode={} target_scope={}",
        if runtime_ref().options.dry_run {
            "dry-run"
        } else {
            "inject"
        },
        runtime_ref().config.target_scope
    );

    let mut f8_was_down = false;
    let mut f9_was_down = false;
    loop {
        dispatch_messages();
        let now = Instant::now();
        if now.duration_since(runtime_ref().last_probe) >= Duration::from_millis(100) {
            probe_target(now);
        }
        if now.duration_since(runtime_ref().last_config_check)
            >= Duration::from_secs(runtime_ref().config.config_reload_secs)
        {
            reload_config_if_changed(now);
        }

        let f8_down = key_down(VK_F8 as i32);
        if f8_down && !f8_was_down {
            let runtime = runtime_mut();
            runtime.user_enabled = !runtime.user_enabled;
            sync_engine(runtime);
            println!(
                "Remote Camera {}",
                if runtime.user_enabled {
                    "已启用"
                } else {
                    "已停用"
                }
            );
        }
        f8_was_down = f8_down;

        let f9_down = key_down(VK_F9 as i32);
        if f9_down && !f9_was_down {
            return;
        }
        f9_was_down = f9_down;
        unsafe { Sleep(runtime_ref().config.poll_interval_ms as u32) };
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && wparam as u32 == WM_MOUSEMOVE && lparam != 0 {
        let event = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        // Ignore only our own SendInput events. Other remote-control clients
        // commonly arrive through the same injected-input path.
        if event.dwExtraInfo != INJECTED_TAG {
            let runtime = runtime_mut();
            if let Some(target) = runtime.target {
                if target.id != 0 && unsafe { GetForegroundWindow() } as usize != target.id {
                    return unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) };
                }
                if runtime.engine.state() == EngineState::Tracking {
                    let output =
                        runtime
                            .engine
                            .on_cursor_position(event.pt.x, event.pt.y, Instant::now());
                    if let Some(output) = output {
                        if output.recenter && !runtime.options.dry_run {
                            set_cursor(target.center_x, target.center_y);
                            runtime.engine.on_recenter_complete(Instant::now());
                        }
                        let has_motion =
                            output.motion.dx.abs() >= 0.5 || output.motion.dy.abs() >= 0.5;
                        if !runtime.options.dry_run && has_motion {
                            send_relative(output.motion.dx, output.motion.dy);
                        }
                        if output.swallow && !runtime.options.dry_run {
                            return 1;
                        }
                    }
                }
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

fn probe_target(now: Instant) {
    let foreground = unsafe { GetForegroundWindow() };
    let target = window_target(foreground);
    let runtime = runtime_mut();
    runtime.last_probe = now;
    let rdp_session = detect_rdp_session();
    if rdp_session != runtime.rdp_session && runtime.options.verbose {
        println!(
            "rdp_session={rdp_session} session_mode={}",
            runtime.config.session_mode
        );
    }
    runtime.rdp_session = rdp_session;
    if target != runtime.target {
        runtime.target = target;
        let changed = runtime.engine.target_changed(target);
        if changed && runtime.options.verbose {
            if let Some(target) = target {
                let process = window_process_name(target.id as HWND).unwrap_or_else(|| "?".into());
                let title = window_title(target.id as HWND).unwrap_or_default();
                println!(
                    "target={} hwnd=0x{:x} process={} title={:?}",
                    runtime.config.target_scope, target.id, process, title
                );
            } else {
                println!("target=none");
            }
        }
    }
    sync_engine(runtime);
}

fn reload_config_if_changed(now: Instant) {
    let path = runtime_ref().config.config_path.clone();
    let modified = std::fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .ok();
    let should_reload = modified.is_some() && modified != runtime_ref().config_modified;
    let runtime = runtime_mut();
    runtime.last_config_check = now;
    if !should_reload {
        return;
    }
    let config = crate::config::config_from_path(path);
    runtime.engine.update_config(config.clone());
    runtime.config = config;
    sync_engine(runtime);
    runtime.config_modified = modified;
    if runtime.options.verbose {
        println!("configuration reloaded");
    }
}

fn sync_engine(runtime: &mut Runtime) {
    let allowed = runtime.user_enabled
        && (runtime.config.session_mode != "rdp"
            || runtime.rdp_session
            || runtime.options.allow_local);
    if allowed != runtime.engine.is_enabled() {
        runtime.engine.set_enabled(allowed);
    }
}

fn window_target(window: HWND) -> Option<TargetWindow> {
    let runtime = runtime_ref();
    if runtime.config.target_scope == "desktop" {
        return virtual_desktop_target();
    }
    // The foreground window is the common case. Some remote-control clients
    // briefly report a helper window, so fall back to a visible top-level
    // window owned by an allowed game process.
    if !window.is_null() && is_target_window(window) {
        return target_from_window(window);
    }
    let mut candidates = Vec::new();
    unsafe {
        EnumWindows(
            Some(collect_target_window),
            &mut candidates as *mut _ as LPARAM,
        );
    }
    candidates.into_iter().next().and_then(target_from_window)
}

unsafe extern "system" fn collect_target_window(window: HWND, lparam: LPARAM) -> i32 {
    if unsafe { IsWindowVisible(window) } == 0 || !is_target_window(window) {
        return 1;
    }
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(window, &mut rect) } == 0
        || rect.right - rect.left < 100
        || rect.bottom - rect.top < 100
    {
        return 1;
    }
    let candidates = unsafe { &mut *(lparam as *mut Vec<HWND>) };
    candidates.push(window);
    1
}

fn target_from_window(window: HWND) -> Option<TargetWindow> {
    let mut rect = RECT::default();
    if unsafe { GetClientRect(window, &mut rect) } == 0 {
        return None;
    }
    let mut center = POINT {
        x: (rect.right - rect.left) / 2,
        y: (rect.bottom - rect.top) / 2,
    };
    if unsafe { ClientToScreen(window, &mut center) } == 0 {
        return None;
    }
    Some(TargetWindow {
        id: window as usize,
        center_x: center.x,
        center_y: center.y,
    })
}

fn virtual_desktop_target() -> Option<TargetWindow> {
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    if width <= 0 || height <= 0 {
        return None;
    }
    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    Some(TargetWindow {
        id: 0,
        center_x: left.saturating_add(width / 2),
        center_y: top.saturating_add(height / 2),
    })
}

fn is_target_window(window: HWND) -> bool {
    let runtime = runtime_ref();
    if runtime.config.target_scope == "desktop" {
        return true;
    }
    let Some(name) = window_process_name(window) else {
        return false;
    };
    if !runtime
        .config
        .process_names
        .iter()
        .any(|allowed| allowed == &name)
    {
        return false;
    }

    // "minecraft" is the historical default, but localized launchers and
    // custom clients often replace the window title entirely. The process
    // filter is the default boundary; a custom title is opt-in.
    if runtime.config.title_contains.is_empty() || runtime.config.title_contains == "minecraft" {
        return true;
    }
    window_title(window)
        .map(|title| {
            title
                .to_ascii_lowercase()
                .contains(&runtime.config.title_contains)
        })
        .unwrap_or(false)
}

fn window_process_name(window: HWND) -> Option<String> {
    let mut pid = 0;
    if unsafe { GetWindowThreadProcessId(window, &mut pid) } == 0 {
        return None;
    }
    process_name(pid)
}

fn window_title(window: HWND) -> Option<String> {
    let mut title = [0u16; 512];
    let length = unsafe { GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32) };
    (length > 0).then(|| String::from_utf16_lossy(&title[..length as usize]))
}

fn process_name(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut buffer = [0u16; 512];
    let mut length = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &mut length,
        )
    };
    unsafe { windows_sys::Win32::Foundation::CloseHandle(process) };
    if result == 0 || length == 0 {
        return None;
    }
    let path = String::from_utf16_lossy(&buffer[..length as usize]);
    path.rsplit(['\\', '/']).next().map(str::to_ascii_lowercase)
}

fn set_cursor(x: i32, y: i32) {
    unsafe { SetCursorPos(x, y) };
}

fn send_relative(dx: f64, dy: f64) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: dx.round() as i32,
                dy: dy.round() as i32,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_MOVE_NOCOALESCE,
                time: 0,
                dwExtraInfo: INJECTED_TAG,
            },
        },
    };
    unsafe {
        let _ = SendInput(1, &input, size_of::<INPUT>() as i32);
    }
}

fn detect_rdp_session() -> bool {
    let metric = unsafe { GetSystemMetrics(SM_REMOTESESSION) } != 0;
    let session = std::env::var("SESSIONNAME")
        .map(|value| value.to_ascii_uppercase().starts_with("RDP-TCP"))
        .unwrap_or(false);
    let client = std::env::var("CLIENTNAME")
        .map(|value| !value.is_empty())
        .unwrap_or(false);
    metric || session || client
}

fn dispatch_messages() {
    let mut message = MSG::default();
    while unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) } != 0 {
        if message.message == WM_QUIT {
            return;
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn key_down(key: i32) -> bool {
    unsafe { GetAsyncKeyState(key) & i16::MIN != 0 }
}

fn runtime_ref() -> &'static Runtime {
    let pointer = *RUNTIME.get().expect("runtime not initialized") as *const Runtime;
    unsafe { &*pointer }
}

fn runtime_mut() -> &'static mut Runtime {
    let pointer = *RUNTIME.get().expect("runtime not initialized") as *mut Runtime;
    unsafe { &mut *pointer }
}
