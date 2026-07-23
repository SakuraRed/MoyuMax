//! 系统托盘图标与菜单。
//!
//! 菜单内容全部来自 SQLite 事实：最近实例按会话时间排序，活动任务摘要
//! 与窗口界面读取同一查询结果，菜单每 3 秒按需重建一次。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use moyumax_core::{AppService, ContentInstallTask, InstallTask, RecentInstanceSummary, TaskState};
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::lifecycle::{
    PENDING_INTENT_EVENT, PendingIntent, ShellCoordinator, confirm_graceful_exit, wake_or_focus,
};
use crate::{LaunchCoordinator, TaskCoordinator};

const TRAY_ID: &str = "main";
const MENU_SHOW: &str = "tray:show";
const MENU_EXIT: &str = "tray:exit";
const MENU_PAUSE_ALL: &str = "tray:pause-all";
const MENU_RESUME_ALL: &str = "tray:resume-all";
const MENU_LAUNCH_PREFIX: &str = "tray:launch:";
const MAX_RECENT_INSTANCES: u32 = 5;
const MAX_TASK_LINES: usize = 3;
const MENU_REFRESH_INTERVAL: Duration = Duration::from_secs(3);

/// 托盘菜单的纯数据模型，与 Tauri 类型解耦以便单元测试。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuModel {
    pub recent_instances: Vec<RecentInstanceSummary>,
    pub task_lines: Vec<String>,
    pub active_task_total: u64,
    pub tasks_paused: bool,
}

impl TrayMenuModel {
    /// 从核心服务加载当前事实。
    pub fn load(service: &AppService) -> Result<Self, String> {
        let recent = service
            .recent_instances(MAX_RECENT_INSTANCES)
            .map_err(|error| error.to_string())?;
        let install_tasks = service
            .list_install_tasks()
            .map_err(|error| error.to_string())?;
        let content_tasks = service
            .list_content_install_tasks()
            .map_err(|error| error.to_string())?;
        let tasks_paused = service.tasks_paused().map_err(|error| error.to_string())?;
        Ok(Self::assemble(
            recent,
            &install_tasks,
            &content_tasks,
            tasks_paused,
        ))
    }

    /// 由任务与实例事实组装菜单模型。活动任务按创建时间排序，最多展示三条。
    pub fn assemble(
        recent_instances: Vec<RecentInstanceSummary>,
        install_tasks: &[InstallTask],
        content_tasks: &[ContentInstallTask],
        tasks_paused: bool,
    ) -> Self {
        let mut entries: Vec<(i64, String, bool)> = Vec::new();
        for task in install_tasks {
            if let Some((line, active)) = install_task_line(task) {
                entries.push((task.created_at_unix_seconds, line, active));
            }
        }
        for task in content_tasks {
            if let Some((line, active)) = content_task_line(task) {
                entries.push((task.created_at_unix_seconds, line, active));
            }
        }
        entries.sort_by_key(|(created_at, _, _)| *created_at);
        let active_task_total = entries.iter().filter(|(_, _, active)| *active).count() as u64;
        let task_lines = entries
            .into_iter()
            .take(MAX_TASK_LINES)
            .map(|(_, line, _)| line)
            .collect();
        Self {
            recent_instances,
            task_lines,
            active_task_total,
            tasks_paused,
        }
    }
}

fn install_task_line(task: &InstallTask) -> Option<(String, bool)> {
    let title = format!("安装「{}」", task.plan.instance_name);
    task_line(
        task.state,
        &title,
        task.progress.completed_bytes,
        task.progress.total_bytes,
    )
}

fn content_task_line(task: &ContentInstallTask) -> Option<(String, bool)> {
    let title = format!("内容「{}」", task.plan.instance_name);
    task_line(
        task.state,
        &title,
        task.progress.completed_bytes,
        task.progress.total_bytes,
    )
}

/// 把任务状态渲染为一行只读摘要。返回 None 表示该任务不进入托盘菜单。
fn task_line(
    state: TaskState,
    title: &str,
    completed: u64,
    total: Option<u64>,
) -> Option<(String, bool)> {
    let (detail, active) = match state {
        TaskState::Queued => ("排队中".to_owned(), true),
        TaskState::Running => {
            let percent = total
                .filter(|total| *total > 0)
                .map(|total| completed.saturating_mul(100) / total);
            match percent {
                Some(percent) => (format!("{percent}%"), true),
                None => ("进行中".to_owned(), true),
            }
        }
        TaskState::Committing => ("正在提交".to_owned(), true),
        TaskState::AwaitingRecovery => ("等待恢复确认".to_owned(), true),
        TaskState::Paused => ("已暂停".to_owned(), false),
        TaskState::Completed | TaskState::Failed | TaskState::Cancelled => return None,
    };
    Some((format!("{title} · {detail}"), active))
}

/// 依据模型构建 Tauri 托盘菜单。
fn build_menu(app: &AppHandle, model: &TrayMenuModel) -> tauri::Result<Menu<Wry>> {
    let mut builder = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id(MENU_SHOW, "显示 MoyuMax").build(app)?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(
            &MenuItemBuilder::with_id("tray:recent-header", "最近实例")
                .enabled(false)
                .build(app)?,
        );
    if model.recent_instances.is_empty() {
        builder = builder.item(
            &MenuItemBuilder::with_id("tray:recent-empty", "暂无实例")
                .enabled(false)
                .build(app)?,
        );
    } else {
        for instance in &model.recent_instances {
            let label = if instance.is_running {
                format!("▶ {}(运行中)", instance.name)
            } else {
                format!("▶ {}", instance.name)
            };
            builder = builder.item(
                &MenuItemBuilder::with_id(format!("{MENU_LAUNCH_PREFIX}{}", instance.id), label)
                    .build(app)?,
            );
        }
    }
    let task_header = if model.tasks_paused {
        "任务已暂停".to_owned()
    } else {
        format!("活动任务({} 个进行中)", model.active_task_total)
    };
    builder = builder.item(&PredefinedMenuItem::separator(app)?).item(
        &MenuItemBuilder::with_id("tray:tasks-header", task_header)
            .enabled(false)
            .build(app)?,
    );
    if model.task_lines.is_empty() {
        builder = builder.item(
            &MenuItemBuilder::with_id("tray:tasks-empty", "无活动任务")
                .enabled(false)
                .build(app)?,
        );
    } else {
        for (index, line) in model.task_lines.iter().enumerate() {
            builder = builder.item(
                &MenuItemBuilder::with_id(format!("tray:task:{index}"), line)
                    .enabled(false)
                    .build(app)?,
            );
        }
    }
    if model.tasks_paused {
        builder =
            builder.item(&MenuItemBuilder::with_id(MENU_RESUME_ALL, "恢复全部任务").build(app)?);
    } else {
        builder = builder.item(
            &MenuItemBuilder::with_id(MENU_PAUSE_ALL, "暂停全部任务")
                .enabled(model.active_task_total > 0)
                .build(app)?,
        );
    }
    builder
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&MenuItemBuilder::with_id(MENU_EXIT, "退出 MoyuMax").build(app)?)
        .build()
}

/// 创建托盘图标并注册事件处理与菜单周期刷新。
pub fn setup_tray(app: &AppHandle, shell: Arc<ShellCoordinator>) -> tauri::Result<()> {
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))
        .map_err(std::io::Error::other)?;
    let initial_model = app
        .try_state::<AppService>()
        .and_then(|service| TrayMenuModel::load(&service).ok())
        .unwrap_or_else(|| TrayMenuModel {
            recent_instances: Vec::new(),
            task_lines: Vec::new(),
            active_task_total: 0,
            tasks_paused: false,
        });
    let menu = build_menu(app, &initial_model)?;
    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("MoyuMax")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| route_menu_event(app, event.id().as_ref()))
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                let _ = wake_or_focus(tray.app_handle(), &shell);
            }
        })
        .build(app)?;
    spawn_menu_refresh(app.clone(), tray);
    Ok(())
}

fn route_menu_event(app: &AppHandle, id: &str) {
    match id {
        MENU_SHOW => {
            if let Some(shell) = app.try_state::<Arc<ShellCoordinator>>() {
                let _ = wake_or_focus(app, &shell);
            }
        }
        MENU_EXIT => {
            let (Some(service), Some(shell), Some(tasks), Some(launches)) = (
                app.try_state::<AppService>(),
                app.try_state::<Arc<ShellCoordinator>>(),
                app.try_state::<TaskCoordinator>(),
                app.try_state::<LaunchCoordinator>(),
            ) else {
                return;
            };
            let requires_confirmation = service
                .exit_impact_summary()
                .map(|impact| impact.requires_confirmation())
                .unwrap_or(true);
            if requires_confirmation {
                // 有影响时交给前端弹窗说明；窗口不存在则先唤醒。
                shell.set_pending_intent(PendingIntent::ExitRequested);
                let _ = wake_or_focus(app, &shell);
                let _ = app.emit(PENDING_INTENT_EVENT, ());
            } else {
                let app_handle = app.clone();
                let service = service.inner().clone();
                let tasks = tasks.inner().clone();
                let launches = launches.inner().clone();
                let shell = Arc::clone(&shell);
                tauri::async_runtime::spawn(async move {
                    let _ =
                        confirm_graceful_exit(app_handle, service, tasks, launches, shell).await;
                });
            }
        }
        MENU_PAUSE_ALL => {
            if let (Some(service), Some(tasks)) = (
                app.try_state::<AppService>(),
                app.try_state::<TaskCoordinator>(),
            ) {
                let _ = tasks.pause_all(&service);
            }
        }
        MENU_RESUME_ALL => {
            if let (Some(service), Some(tasks)) = (
                app.try_state::<AppService>(),
                app.try_state::<TaskCoordinator>(),
            ) {
                let _ = tasks.resume_all(&service);
            }
        }
        id if id.starts_with(MENU_LAUNCH_PREFIX) => {
            let instance_id = id[MENU_LAUNCH_PREFIX.len()..].to_owned();
            if let Some(shell) = app.try_state::<Arc<ShellCoordinator>>() {
                shell.set_pending_intent(PendingIntent::QuickLaunch { instance_id });
                let _ = wake_or_focus(app, &shell);
                let _ = app.emit(PENDING_INTENT_EVENT, ());
            }
        }
        _ => {}
    }
}

/// 每 3 秒按数据库事实重建托盘菜单，仅在模型变化时替换。
fn spawn_menu_refresh(app: AppHandle, tray: TrayIcon<Wry>) {
    let last_model: Arc<Mutex<Option<TrayMenuModel>>> = Arc::new(Mutex::new(None));
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(MENU_REFRESH_INTERVAL).await;
            let Some(service) = app.try_state::<AppService>() else {
                continue;
            };
            let Ok(model) = TrayMenuModel::load(&service) else {
                continue;
            };
            let mut last = last_model
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if last.as_ref() == Some(&model) {
                continue;
            }
            match build_menu(&app, &model) {
                Ok(menu) => {
                    if tray.set_menu(Some(menu)).is_ok() {
                        *last = Some(model);
                    }
                }
                Err(_) => continue,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use moyumax_core::{
        InstallPlan, InstanceIsolation, JavaPlanAction, ResolvedGameVersion, ResolvedLoader,
        TaskProgress,
    };

    fn progress(completed: u64, total: Option<u64>) -> TaskProgress {
        TaskProgress {
            completed_bytes: completed,
            total_bytes: total,
            current_item: None,
            error_summary: None,
            source_detail: None,
        }
    }

    fn install_task(
        id: &str,
        state: TaskState,
        created: i64,
        completed: u64,
        total: Option<u64>,
    ) -> InstallTask {
        InstallTask {
            id: id.to_owned(),
            state,
            current_stage: None,
            plan: InstallPlan {
                schema_version: 1,
                instance_id: "instance".to_owned(),
                instance_name: format!("实例{id}"),
                target_directory: "target".to_owned(),
                shared_store_directory: "store".to_owned(),
                game: ResolvedGameVersion {
                    version: moyumax_core::GameVersionSummary {
                        id: "1.0".to_owned(),
                        release_type: moyumax_core::GameReleaseType::Release,
                        release_time: String::new(),
                        metadata_url: String::new(),
                        metadata_sha1: String::new(),
                        recommended: true,
                    },
                    java_major_version: 21,
                    main_class: String::new(),
                    metadata: serde_json::json!({}),
                    artifacts: Vec::new(),
                    asset_objects_total_bytes: 0,
                },
                loader: ResolvedLoader::Vanilla,
                java_action: JavaPlanAction::Reuse {
                    environment_id: "env".to_owned(),
                    home_directory: "java".to_owned(),
                },
                isolation: InstanceIsolation::Full,
                stages: Vec::new(),
                estimated_download_bytes: 0,
            },
            staging_directory: "staging".to_owned(),
            target_directory: "target".to_owned(),
            created_at_unix_seconds: created,
            updated_at_unix_seconds: created,
            progress: progress(completed, total),
        }
    }

    fn recent(id: &str, name: &str, running: bool) -> RecentInstanceSummary {
        RecentInstanceSummary {
            id: id.to_owned(),
            name: name.to_owned(),
            game_version: "1.0".to_owned(),
            loader_kind: "vanilla".to_owned(),
            last_started_at_unix_seconds: None,
            is_running: running,
        }
    }

    #[test]
    fn tray_model_keeps_active_tasks_sorted_and_capped() {
        let tasks = vec![
            install_task("a", TaskState::Queued, 3, 0, None),
            install_task("b", TaskState::Running, 1, 62, Some(100)),
            install_task("c", TaskState::Completed, 2, 100, Some(100)),
            install_task("d", TaskState::Paused, 4, 10, Some(100)),
            install_task("e", TaskState::Running, 5, 0, None),
            install_task("f", TaskState::Committing, 6, 0, None),
        ];
        let model = TrayMenuModel::assemble(Vec::new(), &tasks, &[], false);
        assert_eq!(model.active_task_total, 4);
        assert_eq!(
            model.task_lines,
            vec![
                "安装「实例b」 · 62%".to_owned(),
                "安装「实例a」 · 排队中".to_owned(),
                "安装「实例d」 · 已暂停".to_owned(),
            ]
        );
    }

    #[test]
    fn tray_model_marks_paused_header_state() {
        let model = TrayMenuModel::assemble(vec![recent("i1", "生存", true)], &[], &[], true);
        assert!(model.tasks_paused);
        assert_eq!(model.active_task_total, 0);
        assert!(model.task_lines.is_empty());
    }
}
