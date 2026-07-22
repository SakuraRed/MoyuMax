use std::{
    collections::HashMap,
    fmt, fs,
    path::{Component, Path, PathBuf},
    process::Stdio,
};

use regex::Regex;
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use tokio::{process::Command, sync::oneshot};
use uuid::Uuid;

use crate::{AppService, CoreError, ManagedInstanceSummary, Result, unix_timestamp};

const WINDOWS_VERSION: &str = "10.0.19045";

#[derive(Clone, PartialEq, Eq)]
pub struct LaunchAccount {
    player_name: String,
    player_uuid: Uuid,
    access_token: String,
    client_id: String,
    xuid: String,
}

impl fmt::Debug for LaunchAccount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LaunchAccount")
            .field("player_name", &self.player_name)
            .field("player_uuid", &self.player_uuid)
            .field("access_token", &"<redacted>")
            .field("client_id", &self.client_id)
            .field("xuid", &self.xuid)
            .finish()
    }
}

impl LaunchAccount {
    pub fn offline(player_name: &str) -> Result<Self> {
        if !(3..=16).contains(&player_name.len())
            || !player_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(CoreError::Launch(
                "本地玩家名称必须是 3-16 位 ASCII 字母、数字或下划线".to_owned(),
            ));
        }
        let mut hasher = Sha1::new();
        hasher.update(b"MoyuMax offline player\0");
        hasher.update(player_name.as_bytes());
        let digest = hasher.finalize();
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        bytes[6] = (bytes[6] & 0x0f) | 0x50;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Ok(Self {
            player_name: player_name.to_owned(),
            player_uuid: Uuid::from_bytes(bytes),
            access_token: "0".to_owned(),
            client_id: String::new(),
            xuid: String::new(),
        })
    }

    #[must_use]
    pub const fn player_uuid(&self) -> Uuid {
        self.player_uuid
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchOptions {
    pub minimum_memory_mib: u32,
    pub maximum_memory_mib: u32,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            minimum_memory_mib: 512,
            maximum_memory_mib: 4_096,
        }
    }
}

#[derive(Clone)]
pub struct PreparedLaunch {
    executable: PathBuf,
    working_directory: PathBuf,
    arguments: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LaunchSessionState {
    Starting,
    Running,
    Completed,
    Failed,
    Stopped,
    Interrupted,
}

impl LaunchSessionState {
    const fn database_value(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Interrupted => "interrupted",
        }
    }

    fn from_database(value: &str) -> Result<Self> {
        match value {
            "starting" => Ok(Self::Starting),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "stopped" => Ok(Self::Stopped),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(CoreError::Launch(format!("未知启动会话状态：{value}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSessionSummary {
    pub id: String,
    pub instance_id: String,
    pub player_name: String,
    pub state: LaunchSessionState,
    pub started_at_unix_seconds: i64,
    pub ended_at_unix_seconds: Option<i64>,
    pub exit_code: Option<i32>,
    pub stdout_path: String,
    pub stderr_path: String,
    pub error_summary: Option<String>,
}

pub struct LaunchExecution {
    session: LaunchSessionSummary,
    prepared: PreparedLaunch,
}

impl fmt::Debug for LaunchExecution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LaunchExecution")
            .field("session", &self.session)
            .field("prepared", &self.prepared)
            .finish()
    }
}

impl LaunchExecution {
    #[must_use]
    pub const fn session(&self) -> &LaunchSessionSummary {
        &self.session
    }
}

impl fmt::Debug for PreparedLaunch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedLaunch")
            .field("executable", &self.executable)
            .field("working_directory", &self.working_directory)
            .field("arguments", &"<redacted>")
            .finish()
    }
}

impl PreparedLaunch {
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    #[must_use]
    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
}

impl AppService {
    pub fn create_launch_execution(
        &self,
        instance_id: &str,
        account: &LaunchAccount,
        options: &LaunchOptions,
    ) -> Result<LaunchExecution> {
        let (instance, runtime, managed_java_home, java_status) =
            self.instance_runtime(instance_id)?;
        let disk_manifest_path = Path::new(&instance.root_directory).join(".moyumax/runtime.json");
        let disk_manifest: Value =
            serde_json::from_slice(&fs::read(&disk_manifest_path).map_err(|error| {
                CoreError::Launch(format!(
                    "无法读取实例运行时清单 {}：{error}",
                    disk_manifest_path.display()
                ))
            })?)?;
        if disk_manifest != runtime {
            return Err(CoreError::Launch(
                "实例运行时清单与数据库快照不一致，请先修复实例".to_owned(),
            ));
        }
        if java_status != "ready" {
            return Err(CoreError::Launch(
                "实例引用的受管 Java 环境尚未就绪".to_owned(),
            ));
        }
        require_indexed_directory(
            &runtime,
            "javaHome",
            Path::new(&managed_java_home),
            "运行时清单与受管 Java 环境索引不一致",
        )?;
        require_indexed_directory(
            &runtime,
            "sharedStore",
            &self.selected_data_directory()?.join("store"),
            "运行时清单与受管共享存储索引不一致",
        )?;
        let prepared = prepare_launch_from_runtime(&instance, &runtime, account, options)?;
        let session_id = Uuid::new_v4().to_string();
        let logs_directory = prepared.working_directory.join("logs/moyumax");
        let stdout_path = logs_directory.join(format!("{session_id}.stdout.log"));
        let stderr_path = logs_directory.join(format!("{session_id}.stderr.log"));
        let session = LaunchSessionSummary {
            id: session_id,
            instance_id: instance.id,
            player_name: account.player_name.clone(),
            state: LaunchSessionState::Starting,
            started_at_unix_seconds: unix_timestamp(),
            ended_at_unix_seconds: None,
            exit_code: None,
            stdout_path: path_string(&stdout_path)?,
            stderr_path: path_string(&stderr_path)?,
            error_summary: None,
        };

        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active = transaction
            .query_row(
                "SELECT id FROM launch_sessions WHERE instance_id = ?1 AND state IN ('starting', 'running') LIMIT 1",
                params![session.instance_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if active.is_some() {
            return Err(CoreError::Launch("该实例已经在运行".to_owned()));
        }
        transaction.execute(
            "INSERT INTO launch_sessions (id, instance_id, player_name, state, started_at_unix_seconds, stdout_path, stderr_path) VALUES (?1, ?2, ?3, 'starting', ?4, ?5, ?6)",
            params![
                session.id,
                session.instance_id,
                session.player_name,
                session.started_at_unix_seconds,
                session.stdout_path,
                session.stderr_path,
            ],
        )?;
        transaction.commit()?;
        Ok(LaunchExecution { session, prepared })
    }

    pub fn list_launch_sessions(&self) -> Result<Vec<LaunchSessionSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, instance_id, player_name, state, started_at_unix_seconds, ended_at_unix_seconds, exit_code, stdout_path, stderr_path, error_summary FROM launch_sessions ORDER BY started_at_unix_seconds DESC, id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, Option<i32>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<String>>(9)?,
            ))
        })?;
        rows.map(|row| {
            let (
                id,
                instance_id,
                player_name,
                state,
                started_at_unix_seconds,
                ended_at_unix_seconds,
                exit_code,
                stdout_path,
                stderr_path,
                error_summary,
            ) = row?;
            Ok(LaunchSessionSummary {
                id,
                instance_id,
                player_name,
                state: LaunchSessionState::from_database(&state)?,
                started_at_unix_seconds,
                ended_at_unix_seconds,
                exit_code,
                stdout_path,
                stderr_path,
                error_summary,
            })
        })
        .collect()
    }

    pub(crate) fn recover_interrupted_launch_sessions(&self) -> Result<()> {
        self.connection()?.execute(
            "UPDATE launch_sessions SET state = 'interrupted', ended_at_unix_seconds = ?1, error_summary = COALESCE(error_summary, '启动器退出时游戏会话仍在运行') WHERE state IN ('starting', 'running')",
            params![unix_timestamp()],
        )?;
        Ok(())
    }

    fn instance_runtime(
        &self,
        instance_id: &str,
    ) -> Result<(ManagedInstanceSummary, Value, String, String)> {
        let connection = self.connection()?;
        let stored = connection
            .query_row(
                "SELECT i.id, i.name, i.game_version, i.loader_kind, i.loader_version, i.root_directory, i.state, r.runtime_json, j.home_directory, j.status FROM instances i JOIN instance_runtime r ON r.instance_id = i.id JOIN managed_java_environments j ON j.id = r.java_environment_id WHERE i.id = ?1",
                params![instance_id],
                |row| {
                    Ok((
                        ManagedInstanceSummary {
                            id: row.get(0)?,
                            name: row.get(1)?,
                            game_version: row.get(2)?,
                            loader_kind: row.get(3)?,
                            loader_version: row.get(4)?,
                            root_directory: row.get(5)?,
                            state: row.get(6)?,
                        },
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| CoreError::Launch("实例或运行时清单不存在".to_owned()))?;
        Ok((
            stored.0,
            serde_json::from_str(&stored.1)?,
            stored.2,
            stored.3,
        ))
    }

    fn mark_launch_running(&self, session_id: &str) -> Result<()> {
        let changed = self.connection()?.execute(
            "UPDATE launch_sessions SET state = 'running' WHERE id = ?1 AND state = 'starting'",
            params![session_id],
        )?;
        if changed != 1 {
            return Err(CoreError::Launch("启动会话无法进入运行状态".to_owned()));
        }
        Ok(())
    }

    fn finish_launch_session(
        &self,
        session_id: &str,
        state: LaunchSessionState,
        exit_code: Option<i32>,
        error_summary: Option<&str>,
    ) -> Result<LaunchSessionSummary> {
        self.connection()?.execute(
            "UPDATE launch_sessions SET state = ?2, ended_at_unix_seconds = ?3, exit_code = ?4, error_summary = ?5 WHERE id = ?1",
            params![
                session_id,
                state.database_value(),
                unix_timestamp(),
                exit_code,
                error_summary,
            ],
        )?;
        self.list_launch_sessions()?
            .into_iter()
            .find(|session| session.id == session_id)
            .ok_or_else(|| CoreError::Launch("启动会话完成后无法读取".to_owned()))
    }
}

pub async fn run_launch_execution(
    service: &AppService,
    execution: LaunchExecution,
    mut stop_receiver: oneshot::Receiver<()>,
) -> Result<LaunchSessionSummary> {
    let session_id = execution.session.id.clone();
    let output_files = (|| -> Result<(fs::File, fs::File)> {
        if let Some(parent) = Path::new(&execution.session.stdout_path).parent() {
            fs::create_dir_all(parent).map_err(|error| {
                CoreError::Launch(format!(
                    "无法创建游戏日志目录 {}：{error}",
                    parent.display()
                ))
            })?;
        }
        let stdout = fs::File::create(&execution.session.stdout_path).map_err(|error| {
            CoreError::Launch(format!(
                "无法创建标准输出日志 {}：{error}",
                execution.session.stdout_path
            ))
        })?;
        let stderr = fs::File::create(&execution.session.stderr_path).map_err(|error| {
            CoreError::Launch(format!(
                "无法创建标准错误日志 {}：{error}",
                execution.session.stderr_path
            ))
        })?;
        Ok((stdout, stderr))
    })();
    let (stdout, stderr) = match output_files {
        Ok(files) => files,
        Err(error) => {
            let summary = error.to_string();
            let _ = service.finish_launch_session(
                &session_id,
                LaunchSessionState::Failed,
                None,
                Some(&summary),
            );
            return Err(error);
        }
    };
    let mut command = Command::new(&execution.prepared.executable);
    command
        .args(&execution.prepared.arguments)
        .current_dir(&execution.prepared.working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command
            .as_std_mut()
            .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let summary = error.to_string();
            let _ = service.finish_launch_session(
                &session_id,
                LaunchSessionState::Failed,
                None,
                Some(&summary),
            );
            return Err(error.into());
        }
    };
    if let Err(error) = service.mark_launch_running(&session_id) {
        let _ = child.kill().await;
        let _ = child.wait().await;
        let summary = error.to_string();
        let _ = service.finish_launch_session(
            &session_id,
            LaunchSessionState::Failed,
            None,
            Some(&summary),
        );
        return Err(error);
    }

    let lifecycle: Result<_> = async {
        tokio::select! {
            status = child.wait() => Ok((status?, false)),
            stop_request = &mut stop_receiver => {
                if stop_request.is_ok() {
                    if let Some(status) = child.try_wait()? {
                        Ok((status, false))
                    } else {
                        child.kill().await?;
                        Ok((child.wait().await?, true))
                    }
                } else {
                    Ok((child.wait().await?, false))
                }
            }
        }
    }
    .await;
    let (status, stopped) = match lifecycle {
        Ok(outcome) => outcome,
        Err(error) => {
            let summary = error.to_string();
            let _ = service.finish_launch_session(
                &session_id,
                LaunchSessionState::Failed,
                None,
                Some(&summary),
            );
            return Err(error);
        }
    };
    let exit_code = status.code();
    if stopped {
        return service.finish_launch_session(
            &session_id,
            LaunchSessionState::Stopped,
            exit_code,
            None,
        );
    }
    if status.success() {
        service.finish_launch_session(&session_id, LaunchSessionState::Completed, exit_code, None)
    } else {
        let summary = exit_code.map_or_else(
            || "游戏进程异常终止且没有退出码".to_owned(),
            |code| format!("游戏进程退出码：{code}"),
        );
        service.finish_launch_session(
            &session_id,
            LaunchSessionState::Failed,
            exit_code,
            Some(&summary),
        )
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeManifest {
    schema_version: u32,
    game_version: String,
    main_class: String,
    java_home: String,
    shared_store: String,
    working_directory: String,
    natives_directory: String,
    classpath: Vec<String>,
    game_metadata: Value,
    loader_profile: Option<Value>,
}

pub fn prepare_launch_from_runtime(
    instance: &ManagedInstanceSummary,
    runtime: &Value,
    account: &LaunchAccount,
    options: &LaunchOptions,
) -> Result<PreparedLaunch> {
    validate_launch_options(options)?;
    if instance.state != "ready" {
        return Err(CoreError::Launch(format!(
            "实例 {} 当前状态不是 ready",
            instance.name
        )));
    }
    let runtime: RuntimeManifest = serde_json::from_value(runtime.clone())?;
    if runtime.schema_version != 1 {
        return Err(CoreError::Launch(format!(
            "不支持运行时清单版本 {}",
            runtime.schema_version
        )));
    }
    if runtime.game_version != instance.game_version || runtime.main_class.trim().is_empty() {
        return Err(CoreError::Launch(
            "运行时清单与实例版本或主类不一致".to_owned(),
        ));
    }

    let instance_root = absolute_directory(&instance.root_directory, "实例目录")?;
    let java_home = absolute_directory(&runtime.java_home, "托管 Java 目录")?;
    let shared_store = absolute_directory(&runtime.shared_store, "共享存储目录")?;
    let executable = java_home.join("bin/java.exe");
    require_file(&executable, "托管 Java")?;
    let working_directory =
        safe_relative_path(&instance_root, &runtime.working_directory, "实例工作目录")?;
    if !working_directory.is_dir() {
        return Err(CoreError::Launch(format!(
            "实例工作目录不存在：{}",
            working_directory.display()
        )));
    }
    let natives_directory =
        safe_relative_path(&instance_root, &runtime.natives_directory, "原生库目录")?;
    if !natives_directory.is_dir() {
        return Err(CoreError::Launch(format!(
            "原生库目录不存在：{}",
            natives_directory.display()
        )));
    }

    let mut classpath_entries =
        resolve_classpath(&runtime.classpath, &runtime.game_version, &shared_store)?;
    let has_modern_native = classpath_entries.iter().any(|path| {
        let text = path.to_string_lossy();
        text.ends_with("natives-windows.jar")
            || text.ends_with("natives-windows-x86_64.jar")
            || text.ends_with("natives-windows-amd64.jar")
    });
    if !has_modern_native && !contains_dll(&natives_directory)? {
        return Err(CoreError::Launch(
            "运行时既没有 Windows x64 native JAR，也没有旧式预解包 DLL".to_owned(),
        ));
    }
    let version_jar = shared_store.join(format!(
        "minecraft/versions/{0}/{0}.jar",
        runtime.game_version
    ));
    if let Some(position) = classpath_entries
        .iter()
        .position(|path| path == &version_jar)
    {
        let version_jar = classpath_entries.remove(position);
        classpath_entries.push(version_jar);
    }
    let classpath = join_classpath(&classpath_entries)?;

    let assets_index_name = runtime
        .game_metadata
        .pointer("/assetIndex/id")
        .and_then(Value::as_str)
        .or_else(|| runtime.game_metadata.get("assets").and_then(Value::as_str))
        .ok_or_else(|| CoreError::Launch("运行时清单缺少资源索引名称".to_owned()))?;
    let assets_root = shared_store.join("minecraft/assets");
    require_file(
        &assets_root
            .join("indexes")
            .join(format!("{assets_index_name}.json")),
        "资源索引",
    )?;
    let logging_path = logging_configuration(&runtime.game_metadata, &shared_store)?;

    let version_name = runtime
        .loader_profile
        .as_ref()
        .and_then(|profile| profile.get("id"))
        .and_then(Value::as_str)
        .unwrap_or(&runtime.game_version);
    let version_type = runtime
        .game_metadata
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("release");
    let mut replacements = HashMap::from([
        ("natives_directory", path_string(&natives_directory)?),
        ("launcher_name", "MoyuMax".to_owned()),
        ("launcher_version", env!("CARGO_PKG_VERSION").to_owned()),
        ("classpath", classpath),
        ("classpath_separator", classpath_separator().to_owned()),
        (
            "library_directory",
            path_string(&shared_store.join("minecraft/libraries"))?,
        ),
        ("auth_player_name", account.player_name.clone()),
        ("profile_name", account.player_name.clone()),
        ("version_name", version_name.to_owned()),
        ("game_directory", path_string(&working_directory)?),
        ("assets_root", path_string(&assets_root)?),
        ("game_assets", path_string(&assets_root)?),
        ("assets_index_name", assets_index_name.to_owned()),
        ("auth_uuid", account.player_uuid.simple().to_string()),
        ("auth_access_token", account.access_token.clone()),
        (
            "auth_session",
            format!("token:{}", account.player_uuid.simple()),
        ),
        ("clientid", account.client_id.clone()),
        ("auth_xuid", account.xuid.clone()),
        ("user_type", "legacy".to_owned()),
        ("user_properties", "{}".to_owned()),
        ("version_type", version_type.to_owned()),
    ]);
    if let Some(path) = &logging_path {
        replacements.insert("path", path_string(path)?);
    }

    let features = HashMap::<String, bool>::new();
    let arguments = runtime
        .game_metadata
        .get("arguments")
        .ok_or_else(|| CoreError::Launch("运行时清单缺少 arguments".to_owned()))?;
    let mut jvm_arguments = vec![
        format!("-Xms{}M", options.minimum_memory_mib),
        format!("-Xmx{}M", options.maximum_memory_mib),
    ];
    if let Some(defaults) = arguments.get("default-user-jvm") {
        let mut values = collect_arguments(defaults, &features)?;
        values.retain(|value| !value.starts_with("-Xms") && !value.starts_with("-Xmx"));
        jvm_arguments.extend(values);
    }
    if let Some(jvm) = arguments.get("jvm") {
        jvm_arguments.extend(collect_arguments(jvm, &features)?);
    }
    if let Some(loader_jvm) = runtime
        .loader_profile
        .as_ref()
        .and_then(|profile| profile.pointer("/arguments/jvm"))
    {
        jvm_arguments.extend(collect_arguments(loader_jvm, &features)?);
    }
    if let Some(logging_argument) = logging_argument(&runtime.game_metadata) {
        jvm_arguments.push(logging_argument.to_owned());
    }
    if !jvm_arguments.iter().any(|argument| argument == "-cp") {
        jvm_arguments.extend(["-cp".to_owned(), "${classpath}".to_owned()]);
    }

    let mut game_arguments = if let Some(game) = arguments.get("game") {
        collect_arguments(game, &features)?
    } else if let Some(legacy) = runtime
        .game_metadata
        .get("minecraftArguments")
        .and_then(Value::as_str)
    {
        legacy.split_whitespace().map(str::to_owned).collect()
    } else {
        return Err(CoreError::Launch("运行时清单缺少游戏参数".to_owned()));
    };
    if let Some(loader_game) = runtime
        .loader_profile
        .as_ref()
        .and_then(|profile| profile.pointer("/arguments/game"))
    {
        game_arguments.extend(collect_arguments(loader_game, &features)?);
    }

    let mut expanded = jvm_arguments
        .into_iter()
        .map(|argument| expand_argument(&argument, &replacements))
        .collect::<Result<Vec<_>>>()?;
    expanded.push(runtime.main_class);
    expanded.extend(
        game_arguments
            .into_iter()
            .map(|argument| expand_argument(&argument, &replacements))
            .collect::<Result<Vec<_>>>()?,
    );

    Ok(PreparedLaunch {
        executable,
        working_directory,
        arguments: expanded,
    })
}

fn validate_launch_options(options: &LaunchOptions) -> Result<()> {
    if options.minimum_memory_mib < 256
        || options.maximum_memory_mib < options.minimum_memory_mib
        || options.maximum_memory_mib > 65_536
    {
        return Err(CoreError::Launch(
            "内存设置必须满足 256 MiB <= 最小值 <= 最大值 <= 65536 MiB".to_owned(),
        ));
    }
    Ok(())
}

fn absolute_directory(value: &str, label: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if !path.is_absolute() || !path.is_dir() {
        return Err(CoreError::Launch(format!(
            "{label}不存在或不是绝对目录：{}",
            path.display()
        )));
    }
    Ok(path)
}

fn require_indexed_directory(
    runtime: &Value,
    field: &str,
    indexed_directory: &Path,
    mismatch_message: &str,
) -> Result<()> {
    let configured = runtime
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| CoreError::Launch(format!("运行时清单缺少 {field}")))?;
    let configured = fs::canonicalize(configured).map_err(|error| {
        CoreError::Launch(format!("运行时清单目录无法访问 {configured}：{error}"))
    })?;
    let indexed = fs::canonicalize(indexed_directory).map_err(|error| {
        CoreError::Launch(format!(
            "受管目录索引无法访问 {}：{error}",
            indexed_directory.display()
        ))
    })?;
    if configured != indexed {
        return Err(CoreError::Launch(mismatch_message.to_owned()));
    }
    Ok(())
}

fn safe_relative_path(root: &Path, relative: &str, label: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(CoreError::Launch(format!(
            "{label}包含不安全路径：{relative}"
        )));
    }
    Ok(root.join(path))
}

fn require_file(path: &Path, label: &str) -> Result<()> {
    if !path.is_file() {
        return Err(CoreError::Launch(format!(
            "{label}文件不存在：{}",
            path.display()
        )));
    }
    Ok(())
}

fn resolve_classpath(
    entries: &[String],
    game_version: &str,
    shared_store: &Path,
) -> Result<Vec<PathBuf>> {
    if entries.is_empty() {
        return Err(CoreError::Launch("运行时 classpath 为空".to_owned()));
    }
    let mut resolved = Vec::with_capacity(entries.len());
    for entry in entries {
        let file_name = Path::new(entry)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(entry);
        if file_name.ends_with("-natives-windows-x86.jar")
            || file_name.ends_with("-natives-windows-arm64.jar")
            || file_name.ends_with("-natives-windows-aarch64.jar")
        {
            return Err(CoreError::Launch(format!(
                "classpath 包含错误架构原生库：{entry}"
            )));
        }
        let path = safe_relative_path(shared_store, entry, "classpath")?;
        require_file(&path, "classpath")?;
        resolved.push(path);
    }
    let expected = format!("minecraft/versions/{game_version}/{game_version}.jar");
    if !entries.iter().any(|entry| entry == &expected) {
        return Err(CoreError::Launch(format!(
            "classpath 缺少游戏客户端：{expected}"
        )));
    }
    Ok(resolved)
}

fn join_classpath(entries: &[PathBuf]) -> Result<String> {
    entries
        .iter()
        .map(|path| path_string(path))
        .collect::<Result<Vec<_>>>()
        .map(|entries| entries.join(classpath_separator()))
}

const fn classpath_separator() -> &'static str {
    if cfg!(windows) { ";" } else { ":" }
}

fn contains_dll(directory: &Path) -> Result<bool> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if contains_dll(&entry.path())? {
                return Ok(true);
            }
        } else if entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn logging_configuration(metadata: &Value, shared_store: &Path) -> Result<Option<PathBuf>> {
    let Some(id) = metadata
        .pointer("/logging/client/file/id")
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let path = safe_relative_path(
        &shared_store.join("minecraft/assets/log_configs"),
        id,
        "日志配置",
    )?;
    require_file(&path, "日志配置")?;
    Ok(Some(path))
}

fn logging_argument(metadata: &Value) -> Option<&str> {
    metadata
        .pointer("/logging/client/argument")
        .and_then(Value::as_str)
}

fn collect_arguments(value: &Value, features: &HashMap<String, bool>) -> Result<Vec<String>> {
    match value {
        Value::String(value) => Ok(vec![value.clone()]),
        Value::Array(values) => {
            let mut arguments = Vec::new();
            for value in values {
                arguments.extend(collect_arguments(value, features)?);
            }
            Ok(arguments)
        }
        Value::Object(object) => {
            if let Some(rules) = object.get("rules")
                && !argument_rules_allow(rules, features)?
            {
                return Ok(Vec::new());
            }
            let value = object
                .get("value")
                .ok_or_else(|| CoreError::Launch("参数对象缺少 value".to_owned()))?;
            collect_arguments(value, features)
        }
        _ => Err(CoreError::Launch(
            "参数值必须是字符串、数组或规则对象".to_owned(),
        )),
    }
}

fn argument_rules_allow(rules: &Value, features: &HashMap<String, bool>) -> Result<bool> {
    let rules = rules
        .as_array()
        .ok_or_else(|| CoreError::Launch("参数 rules 必须是数组".to_owned()))?;
    let mut allowed = false;
    for rule in rules {
        if argument_rule_matches(rule, features)? {
            let action = rule
                .get("action")
                .and_then(Value::as_str)
                .ok_or_else(|| CoreError::Launch("参数规则缺少 action".to_owned()))?;
            allowed = action == "allow";
        }
    }
    Ok(allowed)
}

fn argument_rule_matches(rule: &Value, features: &HashMap<String, bool>) -> Result<bool> {
    if let Some(os) = rule.get("os") {
        if os
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| name != "windows")
        {
            return Ok(false);
        }
        if let Some(arch) = os.get("arch").and_then(Value::as_str) {
            let expression = Regex::new(&format!("^(?:{arch})$"))
                .map_err(|error| CoreError::Launch(format!("参数架构规则无效：{error}")))?;
            if !expression.is_match("x86_64") && !expression.is_match("amd64") {
                return Ok(false);
            }
        }
        if let Some(version) = os.get("version").and_then(Value::as_str) {
            let expression = Regex::new(&format!("^(?:{version})$"))
                .map_err(|error| CoreError::Launch(format!("参数系统规则无效：{error}")))?;
            if !expression.is_match(WINDOWS_VERSION) {
                return Ok(false);
            }
        }
        if let Some(range) = os.get("versionRange") {
            if let Some(minimum) = range.get("min").and_then(Value::as_str)
                && compare_versions(WINDOWS_VERSION, minimum).is_lt()
            {
                return Ok(false);
            }
            if let Some(maximum) = range.get("max").and_then(Value::as_str)
                && compare_versions(WINDOWS_VERSION, maximum).is_ge()
            {
                return Ok(false);
            }
        }
    }
    if let Some(expected) = rule.get("features").and_then(Value::as_object) {
        for (name, expected) in expected {
            let expected = expected
                .as_bool()
                .ok_or_else(|| CoreError::Launch(format!("功能规则 {name} 必须是布尔值")))?;
            if features.get(name).copied().unwrap_or(false) != expected {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let mut left = left
        .split('.')
        .map(|value| value.parse::<u64>().unwrap_or(0));
    let mut right = right
        .split('.')
        .map(|value| value.parse::<u64>().unwrap_or(0));
    loop {
        match (left.next(), right.next()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (left, right) => {
                let ordering = left.unwrap_or(0).cmp(&right.unwrap_or(0));
                if !ordering.is_eq() {
                    return ordering;
                }
            }
        }
    }
}

fn expand_argument(argument: &str, replacements: &HashMap<&str, String>) -> Result<String> {
    let mut expanded = argument.to_owned();
    for (name, value) in replacements {
        expanded = expanded.replace(&format!("${{{name}}}"), value);
    }
    if expanded.contains("${") {
        return Err(CoreError::Launch(format!(
            "启动参数包含未知占位符：{expanded}"
        )));
    }
    Ok(expanded)
}

fn path_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| CoreError::Launch(format!("路径不是有效 Unicode：{}", path.display())))
}
