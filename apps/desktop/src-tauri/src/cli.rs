//! 内置命令行入口（开发者模式）。
//!
//! `moyumax-desktop.exe --cli <command>` 直接进入无窗口 CLI 模式：不启动
//! Tauri、不创建托盘。命令与 GUI 共用同一 `AppService` 事务；输出为单行
//! 版本化 JSON 信封，退出码稳定。凭据、永久删除与安全设置修改不在命令集内。

use std::path::PathBuf;

use moyumax_core::{AppService, BackupTrigger};
use serde_json::{Value, json};

pub const CLI_SCHEMA_VERSION: u16 = 1;
pub const EXIT_OK: i32 = 0;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_RUNTIME: i32 = 3;
pub const EXIT_DISABLED: i32 = 4;

#[derive(Debug)]
pub struct CliOutcome {
    pub exit_code: i32,
    pub envelope: Value,
}

/// 进程级 CLI 入口：解析状态目录、执行命令、打印信封并返回退出码。
pub fn run_cli(argv: Vec<String>) -> i32 {
    attach_parent_console();
    let outcome = (|| -> CliOutcome {
        let state_directory = match cli_state_directory() {
            Ok(directory) => directory,
            Err(error) => {
                return envelope(
                    EXIT_RUNTIME,
                    &argv,
                    Err(("state_directory", error.to_string())),
                );
            }
        };
        let data_directory = match std::env::var_os("MOYUMAX_DATA_DIR") {
            Some(configured) => PathBuf::from(configured),
            None => state_directory.join("data"),
        };
        let service =
            match AppService::open(&state_directory.join("state.sqlite3"), &data_directory) {
                Ok(service) => service,
                Err(error) => {
                    return envelope(EXIT_RUNTIME, &argv, Err(("open_state", error.to_string())));
                }
            };
        execute(&service, &argv)
    })();
    println!(
        "{}",
        serde_json::to_string(&outcome.envelope).unwrap_or_else(|_| {
            "{\"schemaVersion\":1,\"command\":[],\"ok\":false,\"error\":{\"code\":\"serialize\",\"message\":\"输出序列化失败\"}}".to_owned()
        })
    );
    outcome.exit_code
}

/// 命令执行层（供 BDD 直接调用）。
pub fn execute(service: &AppService, argv: &[String]) -> CliOutcome {
    match service.cli_enabled() {
        Ok(true) => {}
        Ok(false) => {
            return envelope(
                EXIT_DISABLED,
                argv,
                Err((
                    "cli_disabled",
                    "内置 CLI 未启用：请在设置 → 开发者中开启内置命令行后重试".to_owned(),
                )),
            );
        }
        Err(error) => {
            return envelope(EXIT_RUNTIME, argv, Err(("read_setting", error.to_string())));
        }
    }
    let dry_run = argv.iter().any(|arg| arg == "--dry-run");
    let mut positional = Vec::new();
    let mut skip_next = false;
    for arg in argv {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--instance" {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--") {
            continue;
        }
        positional.push(arg.as_str());
    }
    let instance_option = argv
        .windows(2)
        .find(|pair| pair[0] == "--instance")
        .map(|pair| pair[1].clone());
    match positional.as_slice() {
        ["instances", "list"] => wrap(argv, service.list_instances(), |instances| json!(instances)),
        ["tasks", "list"] => wrap(argv, list_all_tasks(service), |tasks| tasks),
        ["tasks", "pause-all"] => set_tasks_paused(service, argv, true, dry_run),
        ["tasks", "resume-all"] => set_tasks_paused(service, argv, false, dry_run),
        ["backups", "list"] => wrap(
            argv,
            service.list_world_backups(instance_option.as_deref()),
            |backups| json!(backups),
        ),
        ["backups", "create"] => create_backup(service, argv, instance_option.as_deref(), dry_run),
        _ => envelope(EXIT_USAGE, argv, Err(("usage", usage_text()))),
    }
}

fn usage_text() -> String {
    "用法：moyumax-desktop.exe --cli <命令> [--dry-run]\n\
     命令：\n\
     instances list\n\
     tasks list | tasks pause-all | tasks resume-all\n\
     backups list [--instance <id>] | backups create --instance <id>"
        .to_owned()
}

fn list_all_tasks(service: &AppService) -> moyumax_core::Result<Value> {
    let install = service.list_install_tasks()?;
    let content = service.list_content_install_tasks()?;
    Ok(json!({
        "installTasks": install,
        "contentTasks": content,
    }))
}

fn set_tasks_paused(
    service: &AppService,
    argv: &[String],
    paused: bool,
    dry_run: bool,
) -> CliOutcome {
    let action = if paused { "pause_all" } else { "resume_all" };
    if dry_run {
        return envelope(
            EXIT_OK,
            argv,
            Ok(json!({
                "dryRun": true,
                "action": action,
                "currentlyPaused": service.tasks_paused().unwrap_or(false),
            })),
        );
    }
    let result = if paused {
        service.set_tasks_paused(true)
    } else {
        service.set_tasks_paused(false)
    };
    match result {
        Ok(()) => envelope(
            EXIT_OK,
            argv,
            Ok(json!({ "action": action, "paused": paused })),
        ),
        Err(error) => envelope(EXIT_RUNTIME, argv, Err(("write", error.to_string()))),
    }
}

fn create_backup(
    service: &AppService,
    argv: &[String],
    instance_id: Option<&str>,
    dry_run: bool,
) -> CliOutcome {
    let Some(instance_id) = instance_id else {
        return envelope(
            EXIT_USAGE,
            argv,
            Err(("usage", "backups create 需要 --instance <id>".to_owned())),
        );
    };
    let worlds = match service.list_instance_worlds(instance_id) {
        Ok(worlds) => worlds,
        Err(error) => return envelope(EXIT_RUNTIME, argv, Err(("instance", error.to_string()))),
    };
    if dry_run {
        return envelope(
            EXIT_OK,
            argv,
            Ok(json!({
                "dryRun": true,
                "action": "create_world_backup",
                "instanceId": instance_id,
                "worlds": worlds,
            })),
        );
    }
    match service.create_world_backup(instance_id, BackupTrigger::Manual, None) {
        Ok(backup) => envelope(
            EXIT_OK,
            argv,
            Ok(json!({
                "backupId": backup.id,
                "state": backup.state,
                "worldCount": backup.world_count,
                "archiveBytes": backup.archive_bytes,
            })),
        ),
        Err(error) => envelope(EXIT_RUNTIME, argv, Err(("backup", error.to_string()))),
    }
}

fn wrap<T: serde::Serialize>(
    argv: &[String],
    result: moyumax_core::Result<T>,
    map: impl FnOnce(T) -> Value,
) -> CliOutcome {
    match result {
        Ok(value) => envelope(EXIT_OK, argv, Ok(map(value))),
        Err(error) => envelope(EXIT_RUNTIME, argv, Err(("query", error.to_string()))),
    }
}

fn envelope(exit_code: i32, argv: &[String], result: Result<Value, (&str, String)>) -> CliOutcome {
    let base = json!({
        "schemaVersion": CLI_SCHEMA_VERSION,
        "command": argv,
    });
    let envelope = match result {
        Ok(data) => {
            let mut envelope = base;
            envelope["ok"] = json!(true);
            envelope["data"] = data;
            envelope
        }
        Err((code, message)) => {
            let mut envelope = base;
            envelope["ok"] = json!(false);
            envelope["error"] = json!({ "code": code, "message": message });
            envelope
        }
    };
    CliOutcome {
        exit_code,
        envelope,
    }
}

fn cli_state_directory() -> Result<PathBuf, std::io::Error> {
    if let Some(configured) = std::env::var_os("MOYUMAX_STATE_DIR") {
        return Ok(PathBuf::from(configured));
    }
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("LOCALAPPDATA 不可用"))?;
    Ok(local_app_data.join("io.github.sakurared.moyumax"))
}

#[cfg(windows)]
fn attach_parent_console() {
    // Release 构建是窗口子系统进程；附加父控制台才能输出 JSON。
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(
            windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
        );
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}
