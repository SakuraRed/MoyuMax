//! 窗口生命周期、托盘唤醒与优雅退出编排。
//!
//! 本模块只依赖 Tauri 应用句柄与 `moyumax-core` 服务，不承载业务规则。
//! 关键语义：
//! - 最小化到托盘会销毁主 WebView 窗口，仅保留后台核心与托盘图标。
//! - 从托盘唤醒时按 `tauri.conf.json` 相同配置重建窗口，并记录启动方式。
//! - 优雅退出先中断活动下载（转为可恢复暂停）、安全终止游戏会话，
//!   等待收敛后再退出进程，避免损坏受管数据。

use std::{
    fs::OpenOptions,
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use moyumax_core::AppService;
use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::{LaunchCoordinator, TaskCoordinator};

pub const MAIN_WINDOW_LABEL: &str = "main";
pub const CLOSE_REQUESTED_EVENT: &str = "moyumax://close-requested";
pub const PENDING_INTENT_EVENT: &str = "moyumax://pending-intent";
const EXIT_CONVERGE_TIMEOUT: Duration = Duration::from_secs(120);
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// 隐藏窗口保留界面的最长时间，超过后销毁 WebView 释放内存。
const IDLE_DESTROY_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowStartupKind {
    Cold,
    Wake,
}

/// 托盘或窗口动作留给前端处理的意图。前端启动或收到事件后取出并消费。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PendingIntent {
    #[serde(rename_all = "camelCase")]
    QuickLaunch {
        instance_id: String,
    },
    ExitRequested,
}

/// 发布冒烟性能采样（`MOYUMAX_SMOKE=1` 时启用）的事件流。
#[derive(Debug)]
pub struct SmokeTrace {
    start: Instant,
    file: Mutex<std::fs::File>,
}

impl SmokeTrace {
    fn new(state_directory: &std::path::Path) -> std::io::Result<Self> {
        let path = state_directory.join("moyumax-smoke-trace.jsonl");
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        Ok(Self {
            start: Instant::now(),
            file: Mutex::new(file),
        })
    }

    fn log(&self, event: &str) {
        let elapsed = self.start.elapsed().as_millis();
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "{{\"event\":\"{event}\",\"ms\":{elapsed}}}");
            let _ = file.flush();
        }
    }
}

/// 壳层窗口状态与退出编排共享状态。
#[derive(Debug)]
pub struct ShellCoordinator {
    window_alive: AtomicBool,
    minimizing: AtomicBool,
    exiting: AtomicBool,
    startup_kind: Mutex<WindowStartupKind>,
    wake_ipc_pending: AtomicBool,
    hidden_since: Mutex<Option<Instant>>,
    pending_intent: Mutex<Option<PendingIntent>>,
    smoke: Option<SmokeTrace>,
}

impl ShellCoordinator {
    pub fn new(smoke_enabled: bool, state_directory: &std::path::Path) -> Self {
        let smoke = smoke_enabled
            .then(|| SmokeTrace::new(state_directory).ok())
            .flatten();
        Self {
            window_alive: AtomicBool::new(true),
            minimizing: AtomicBool::new(false),
            exiting: AtomicBool::new(false),
            startup_kind: Mutex::new(WindowStartupKind::Cold),
            wake_ipc_pending: AtomicBool::new(false),
            hidden_since: Mutex::new(None),
            pending_intent: Mutex::new(None),
            smoke,
        }
    }

    pub fn trace(&self, event: &str) {
        if let Some(smoke) = &self.smoke {
            smoke.log(event);
        }
    }

    pub fn is_exiting(&self) -> bool {
        self.exiting.load(Ordering::Acquire)
    }

    pub fn begin_exit(&self) {
        self.exiting.store(true, Ordering::Release);
    }

    pub fn is_minimizing(&self) -> bool {
        self.minimizing.load(Ordering::Acquire)
    }

    pub fn startup_kind(&self) -> WindowStartupKind {
        *self
            .startup_kind
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn set_pending_intent(&self, intent: PendingIntent) {
        *self
            .pending_intent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(intent);
    }

    pub fn take_pending_intent(&self) -> Option<PendingIntent> {
        self.pending_intent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    pub fn on_window_destroyed(&self) {
        self.window_alive.store(false, Ordering::Release);
        self.minimizing.store(false, Ordering::Release);
        *self
            .hidden_since
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        self.trace("window_destroyed");
    }

    fn hidden_since(&self) -> Option<Instant> {
        *self
            .hidden_since
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// 唤醒后的首个 bootstrap IPC 到达时记录一次冒烟样本。
    pub fn note_bootstrap_ipc(&self) {
        self.trace("bootstrap_call");
        if self.wake_ipc_pending.swap(false, Ordering::AcqRel) {
            self.trace("bootstrap_ipc");
        }
    }
}

/// 按 `tauri.conf.json` 的窗口配置重建主窗口。修改窗口配置时必须同步此处。
pub fn build_main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, WebviewUrl::App("index.html".into()))
        .title("MoyuMax")
        .inner_size(1280.0, 800.0)
        .min_inner_size(960.0, 600.0)
        .center()
        .decorations(false)
        .resizable(true)
        .fullscreen(false)
        .transparent(false)
        .shadow(true)
        .build()
        .map_err(|error| format!("无法重建主窗口：{error}"))
}

/// 隐藏主窗口进入托盘后台模式。
///
/// 实测 Release 重建 WebView 约需 330–360 ms，超过 250 ms 唤醒预算，
/// 因此采用混合策略：先隐藏保留界面保证快速唤醒，空闲超时后再销毁 WebView
/// 释放内存（销毁后后台私有内存实测约 12 MiB，满足 80/120 MiB 预算）。
pub fn minimize_to_tray(app: &AppHandle, shell: &ShellCoordinator) -> Result<(), String> {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return Ok(());
    };
    shell.trace("window_hidden");
    window
        .hide()
        .map_err(|error| format!("无法隐藏主窗口：{error}"))?;
    *shell
        .hidden_since
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Instant::now());
    Ok(())
}

/// 空闲超时后销毁隐藏的窗口，释放 WebView 内存。由后台定时任务与冒烟驱动调用。
pub fn destroy_hidden_window(app: &AppHandle, shell: &ShellCoordinator) -> Result<(), String> {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return Ok(());
    };
    shell.minimizing.store(true, Ordering::Release);
    window
        .close()
        .map_err(|error| format!("无法销毁隐藏窗口：{error}"))
}

/// 从托盘唤醒：窗口存在则前置显示（快速路径），已销毁则按原配置重建。
pub fn wake_or_focus(app: &AppHandle, shell: &ShellCoordinator) -> Result<(), String> {
    shell.trace("wake_trigger");
    *shell
        .hidden_since
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    if shell.is_minimizing() {
        // 等待进行中的销毁完成，避免窗口标签冲突。
        for _ in 0..80 {
            if app.get_webview_window(MAIN_WINDOW_LABEL).is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        shell.trace("wake_shown");
        return Ok(());
    }
    *shell
        .startup_kind
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = WindowStartupKind::Wake;
    shell.wake_ipc_pending.store(true, Ordering::Release);
    let window = build_main_window(app)?;
    shell.window_alive.store(true, Ordering::Release);
    shell.trace("wake_window_built");
    let _ = window.set_focus();
    Ok(())
}

/// 空闲隐藏超时后销毁 WebView 的周期任务。
pub fn spawn_idle_destroy_task(app: AppHandle, shell: Arc<ShellCoordinator>) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let Some(hidden) = shell.hidden_since() else {
                continue;
            };
            if hidden.elapsed() < IDLE_DESTROY_TIMEOUT {
                continue;
            }
            let _ = destroy_hidden_window(&app, &shell);
        }
    });
}

/// 优雅退出：中断活动下载为可恢复暂停、请求安全终止游戏会话，
/// 等待运行中会话与执行中任务收敛后退出进程。超时返回错误由前端提供强制退出。
pub async fn confirm_graceful_exit(
    app: AppHandle,
    service: AppService,
    tasks: TaskCoordinator,
    launches: LaunchCoordinator,
    shell: Arc<ShellCoordinator>,
) -> Result<(), String> {
    tasks.interrupt_running_for_exit();
    let impact = service.exit_impact_summary().map_err(|e| e.to_string())?;
    for session in &impact.running_sessions {
        let _ = launches.request_stop(&session.instance_id);
    }
    let deadline = Instant::now() + EXIT_CONVERGE_TIMEOUT;
    loop {
        let impact = service.exit_impact_summary().map_err(|e| e.to_string())?;
        if impact.running_sessions.is_empty() && impact.executing_tasks() == 0 {
            break;
        }
        if Instant::now() >= deadline {
            return Err(
                "等待游戏安全终止与任务收敛超时；可以强制退出，但备份或下载可能未完成".to_owned(),
            );
        }
        tokio::time::sleep(EXIT_POLL_INTERVAL).await;
    }
    shell.begin_exit();
    app.exit(0);
    Ok(())
}

/// `MOYUMAX_SMOKE=1` 时执行最小化→唤醒循环，产出快速与慢速唤醒性能样本后退出。
pub fn spawn_smoke_driver(app: AppHandle, shell: Arc<ShellCoordinator>) {
    let cycles = std::env::var("MOYUMAX_SMOKE_CYCLES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(10);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        // 快速路径：隐藏保留界面后唤醒。
        for _ in 0..cycles {
            if minimize_to_tray(&app, &shell).is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1200)).await;
            if wake_or_focus(&app, &shell).is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1200)).await;
        }
        // 慢速路径：销毁 WebView 后重建,验证后台内存与重建唤醒耗时。
        for _ in 0..3_u32 {
            if minimize_to_tray(&app, &shell).is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(600)).await;
            if destroy_hidden_window(&app, &shell).is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
            if wake_or_focus(&app, &shell).is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        shell.trace("smoke_done");
        tokio::time::sleep(Duration::from_millis(500)).await;
        shell.begin_exit();
        app.exit(0);
    });
}
