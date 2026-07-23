use std::{fs, path::Path};

use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    AppService, CoreError, FetchReport, GameVersionSummary, OnboardingSelection, Result,
    SourceAttempt, SourceChannel, unix_timestamp,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArtifactKind {
    VersionMetadata,
    GameClient,
    Library,
    LoaderLibrary,
    NativeLibrary,
    AssetIndex,
    AssetObject,
    LoggingConfiguration,
    JavaArchive,
    ContentMod,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedArtifact {
    pub kind: ArtifactKind,
    pub relative_path: String,
    pub url: String,
    pub size: u64,
    pub sha1: Option<String>,
    pub sha256: Option<String>,
    #[serde(default)]
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedGameVersion {
    pub version: GameVersionSummary,
    pub java_major_version: u16,
    pub main_class: String,
    pub metadata: Value,
    pub artifacts: Vec<ResolvedArtifact>,
    pub asset_objects_total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JavaDistribution {
    AzulZulu,
}

impl JavaDistribution {
    pub(crate) fn database_value(self) -> &'static str {
        match self {
            Self::AzulZulu => "azul-zulu",
        }
    }

    pub(crate) fn directory_name(self) -> &'static str {
        self.database_value()
    }

    pub(crate) fn from_database(value: &str) -> Result<Self> {
        match value {
            "azul-zulu" => Ok(Self::AzulZulu),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知 Java 发行版：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JavaArchitecture {
    X64,
}

impl JavaArchitecture {
    pub(crate) fn database_value(self) -> &'static str {
        match self {
            Self::X64 => "x64",
        }
    }

    pub(crate) fn from_database(value: &str) -> Result<Self> {
        match value {
            "x64" => Ok(Self::X64),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知 Java 架构：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JavaEnvironmentStatus {
    Planned,
    Installing,
    Ready,
    Missing,
    Failed,
    Deleted,
}

impl JavaEnvironmentStatus {
    pub(crate) fn database_value(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Installing => "installing",
            Self::Ready => "ready",
            Self::Missing => "missing",
            Self::Failed => "failed",
            Self::Deleted => "deleted",
        }
    }

    pub(crate) fn from_database(value: &str) -> Result<Self> {
        match value {
            "planned" => Ok(Self::Planned),
            "installing" => Ok(Self::Installing),
            "ready" => Ok(Self::Ready),
            "missing" => Ok(Self::Missing),
            "failed" => Ok(Self::Failed),
            "deleted" => Ok(Self::Deleted),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知 Java 环境状态：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedJavaEnvironment {
    pub id: String,
    pub distribution: JavaDistribution,
    pub full_version: String,
    pub architecture: JavaArchitecture,
    pub home_directory: String,
    pub status: JavaEnvironmentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedJavaPackage {
    pub distribution: JavaDistribution,
    pub full_version: String,
    pub architecture: JavaArchitecture,
    pub package_uuid: String,
    pub artifact: ResolvedArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ResolvedLoader {
    Vanilla,
    Fabric {
        version: String,
        stable: bool,
        profile_url: String,
        profile_sha256: String,
        profile: Value,
    },
    Quilt {
        version: String,
        stable: bool,
        profile_url: String,
        profile_sha256: String,
        profile: Value,
    },
    Forge {
        version: String,
        installer_url: String,
        installer_sha1: String,
        installer_size: u64,
        install_profile: Value,
        version_json: Value,
    },
    NeoForge {
        version: String,
        installer_url: String,
        installer_sha1: String,
        installer_size: u64,
        install_profile: Value,
        version_json: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LoaderChoice {
    Vanilla,
    Fabric { version: String },
    Quilt { version: String },
    Forge { version: String },
    NeoForge { version: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstanceIsolation {
    Full,
    SharedBase,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSelection {
    pub instance_name: String,
    pub game_version: GameVersionSummary,
    pub loader: LoaderChoice,
    pub isolation: InstanceIsolation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedInstallRequest {
    pub instance_name: String,
    pub game: ResolvedGameVersion,
    pub loader: ResolvedLoader,
    pub java: ResolvedJavaPackage,
    pub isolation: InstanceIsolation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallStage {
    Prepare,
    DownloadGameFiles,
    VerifyFiles,
    InstallGameEnvironment,
    ApplyLoader,
    CommitChanges,
    CreateRollbackPoint,
}

impl InstallStage {
    pub(crate) fn database_value(self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::DownloadGameFiles => "download_game_files",
            Self::VerifyFiles => "verify_files",
            Self::InstallGameEnvironment => "install_game_environment",
            Self::ApplyLoader => "apply_loader",
            Self::CommitChanges => "commit_changes",
            Self::CreateRollbackPoint => "create_rollback_point",
        }
    }

    pub(crate) fn from_database(value: &str) -> Result<Self> {
        match value {
            "prepare" => Ok(Self::Prepare),
            "download_game_files" => Ok(Self::DownloadGameFiles),
            "verify_files" => Ok(Self::VerifyFiles),
            "install_game_environment" => Ok(Self::InstallGameEnvironment),
            "apply_loader" => Ok(Self::ApplyLoader),
            "commit_changes" => Ok(Self::CommitChanges),
            "create_rollback_point" => Ok(Self::CreateRollbackPoint),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知安装阶段：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskState {
    Queued,
    Running,
    Committing,
    Paused,
    AwaitingRecovery,
    Failed,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryDecision {
    Resume,
    Discard,
}

impl TaskState {
    pub(crate) fn database_value(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Committing => "committing",
            Self::Paused => "paused",
            Self::AwaitingRecovery => "awaiting_recovery",
            Self::Failed => "failed",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    pub(crate) fn from_database(value: &str) -> Result<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "committing" => Ok(Self::Committing),
            "paused" => Ok(Self::Paused),
            "awaiting_recovery" => Ok(Self::AwaitingRecovery),
            "failed" => Ok(Self::Failed),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(CoreError::InvalidStoredState(format!(
                "未知任务状态：{value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum JavaPlanAction {
    Install {
        environment_id: String,
        target_directory: String,
        package: ResolvedArtifact,
    },
    Reuse {
        environment_id: String,
        home_directory: String,
    },
    AwaitExistingInstall {
        environment_id: String,
        target_directory: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlan {
    pub schema_version: u16,
    pub instance_id: String,
    pub instance_name: String,
    pub target_directory: String,
    pub shared_store_directory: String,
    pub game: ResolvedGameVersion,
    pub loader: ResolvedLoader,
    pub java_action: JavaPlanAction,
    pub isolation: InstanceIsolation,
    pub stages: Vec<InstallStage>,
    pub estimated_download_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallTask {
    pub id: String,
    pub state: TaskState,
    pub current_stage: Option<InstallStage>,
    pub plan: InstallPlan,
    pub staging_directory: String,
    pub target_directory: String,
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub paused_by: Option<String>,
    pub progress: TaskProgress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    pub completed_bytes: u64,
    pub total_bytes: Option<u64>,
    pub current_item: Option<String>,
    pub error_summary: Option<String>,
    #[serde(default)]
    pub source_detail: Option<TaskSourceDetail>,
}

/// 任务最近一次下载的来源详情:真实来源、尝试历史与分段状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSourceDetail {
    pub final_label: String,
    pub channel: SourceChannel,
    pub attempts: Vec<SourceAttempt>,
    pub segmented: bool,
    pub segment_count: u32,
    pub degraded_reason: Option<String>,
}

impl TaskSourceDetail {
    /// 汇总一批工件的下载报告,作为任务级来源事实。
    #[must_use]
    pub fn summarize(reports: &[&FetchReport]) -> Option<Self> {
        let last = reports.iter().rev().find(|report| !report.reused_local)?;
        let attempts: Vec<SourceAttempt> = reports
            .iter()
            .flat_map(|report| report.attempts.iter().cloned())
            .collect();
        let degraded_reason = reports
            .iter()
            .find_map(|report| report.degraded_reason.clone());
        Some(Self {
            final_label: last.final_label.clone(),
            channel: last.channel,
            attempts,
            segmented: reports.iter().any(|report| report.segmented),
            segment_count: reports
                .iter()
                .map(|report| report.segment_count)
                .max()
                .unwrap_or(0),
            degraded_reason,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedInstanceSummary {
    pub id: String,
    pub name: String,
    pub game_version: String,
    pub loader_kind: String,
    pub loader_version: Option<String>,
    pub root_directory: String,
    pub state: String,
}

impl AppService {
    pub fn enqueue_install_task(&self, request: &ResolvedInstallRequest) -> Result<InstallTask> {
        validate_install_request(request)?;
        let data_directory = self.selected_data_directory()?;
        let task_id = Uuid::new_v4().to_string();
        let instance_id = Uuid::new_v4().to_string();
        let staging_directory = data_directory
            .join(".staging")
            .join("install")
            .join(&task_id);
        let target_directory = data_directory.join("instances").join(&instance_id);
        let shared_store_directory = data_directory.join("store");

        fs::create_dir_all(&staging_directory)?;
        fs::create_dir_all(&shared_store_directory)?;

        let result = self.insert_install_task(
            request,
            &task_id,
            &instance_id,
            &staging_directory,
            &target_directory,
            &shared_store_directory,
        );
        if result.is_err() {
            let _ = fs::remove_dir(&staging_directory);
        }
        result
    }

    pub fn list_install_tasks(&self) -> Result<Vec<InstallTask>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT task.id, task.state, task.current_stage, task.plan_json,
                   task.staging_directory, task.target_directory,
                   task.created_at_unix_seconds, task.updated_at_unix_seconds,
                   COALESCE(progress.completed_bytes, 0), progress.total_bytes,
                   progress.current_item, progress.error_summary, progress.source_detail,
                   task.priority, task.paused_by
            FROM install_tasks
            AS task
            LEFT JOIN task_progress AS progress ON progress.task_id = task.id
            ORDER BY task.created_at_unix_seconds, task.id
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, Option<i64>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, i64>(13)?,
                row.get::<_, Option<String>>(14)?,
            ))
        })?;

        rows.map(|row| {
            let (
                id,
                state,
                stage,
                plan,
                staging,
                target,
                created_at,
                updated_at,
                completed_bytes,
                total_bytes,
                current_item,
                error_summary,
                source_detail,
                priority,
                paused_by,
            ) = row?;
            Ok(InstallTask {
                id,
                state: TaskState::from_database(&state)?,
                current_stage: stage
                    .as_deref()
                    .map(InstallStage::from_database)
                    .transpose()?,
                plan: serde_json::from_str(&plan)?,
                staging_directory: staging,
                target_directory: target,
                created_at_unix_seconds: created_at,
                updated_at_unix_seconds: updated_at,
                priority,
                paused_by,
                progress: TaskProgress {
                    completed_bytes: sqlite_unsigned(completed_bytes, "已完成字节数")?,
                    total_bytes: total_bytes
                        .map(|value| sqlite_unsigned(value, "总字节数"))
                        .transpose()?,
                    current_item,
                    error_summary,
                    source_detail: source_detail
                        .map(|json| serde_json::from_str(&json))
                        .transpose()?,
                },
            })
        })
        .collect()
    }

    pub fn mark_install_task_running(&self, task_id: &str) -> Result<()> {
        let changed = self.connection()?.execute(
            "
            UPDATE install_tasks
            SET state = ?2, updated_at_unix_seconds = ?3
            WHERE id = ?1 AND state IN ('queued', 'paused', 'awaiting_recovery')
            ",
            params![
                task_id,
                TaskState::Running.database_value(),
                unix_timestamp()
            ],
        )?;
        if changed == 0 {
            return Err(CoreError::InvalidInstallRequest(
                "任务不存在或当前状态不能开始运行".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn retry_failed_install_task(&self, task_id: &str) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "
            UPDATE install_tasks
            SET state = 'queued', current_stage = 'prepare', updated_at_unix_seconds = ?2
            WHERE id = ?1 AND state = 'failed'
            ",
            params![task_id, unix_timestamp()],
        )?;
        if changed == 0 {
            return Err(CoreError::InvalidInstallRequest(
                "任务不存在或当前状态不能重试".to_owned(),
            ));
        }
        transaction.execute(
            "
            UPDATE task_progress
            SET current_item = '等待重试执行', error_summary = NULL
            WHERE task_id = ?1
            ",
            params![task_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// 下载被中断后把任务标记为可恢复的暂停状态。
    /// `paused_by` 区分全局暂停（`global`）与用户单任务暂停（`user`）,
    /// 全局恢复只重入被全局暂停打断的任务。
    pub fn mark_install_task_paused(&self, task_id: &str, paused_by: &str) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "
            UPDATE install_tasks
            SET state = 'paused', paused_by = ?3, updated_at_unix_seconds = ?2
            WHERE id = ?1 AND state IN ('queued', 'running')
            ",
            params![task_id, unix_timestamp(), paused_by],
        )?;
        if changed == 0 {
            return Err(CoreError::InvalidInstallRequest(
                "任务不存在或当前状态不能暂停".to_owned(),
            ));
        }
        transaction.execute(
            "
            UPDATE task_progress
            SET current_item = '已暂停，可在恢复后继续', error_summary = NULL
            WHERE task_id = ?1
            ",
            params![task_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// 全局恢复：只把被全局暂停打断的任务重新入队，按优先级与创建时间返回。
    pub fn requeue_paused_install_tasks(&self) -> Result<Vec<String>> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut statement = transaction.prepare(
            "
            SELECT id FROM install_tasks
            WHERE state = 'paused' AND paused_by = 'global'
            ORDER BY priority DESC, created_at_unix_seconds, id
            ",
        )?;
        let task_ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);
        if task_ids.is_empty() {
            return Ok(Vec::new());
        }
        transaction.execute(
            "
            UPDATE task_progress
            SET current_item = '等待恢复执行', error_summary = NULL
            WHERE task_id IN (SELECT id FROM install_tasks WHERE state = 'paused' AND paused_by = 'global')
            ",
            [],
        )?;
        transaction.execute(
            "
            UPDATE install_tasks
            SET state = 'queued', paused_by = NULL, current_stage = 'prepare', updated_at_unix_seconds = ?1
            WHERE state = 'paused' AND paused_by = 'global'
            ",
            params![unix_timestamp()],
        )?;
        transaction.commit()?;
        Ok(task_ids)
    }

    /// 单任务恢复：任意暂停来源的任务重新入队。
    pub fn requeue_paused_install_task(&self, task_id: &str) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "
            UPDATE install_tasks
            SET state = 'queued', paused_by = NULL, current_stage = 'prepare', updated_at_unix_seconds = ?2
            WHERE id = ?1 AND state = 'paused'
            ",
            params![task_id, unix_timestamp()],
        )?;
        if changed == 0 {
            return Err(CoreError::InvalidInstallRequest(
                "任务不存在或当前不是暂停状态".to_owned(),
            ));
        }
        transaction.execute(
            "
            UPDATE task_progress
            SET current_item = '等待恢复执行', error_summary = NULL
            WHERE task_id = ?1
            ",
            params![task_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// 调整排队任务的优先级；只影响后续出队顺序。
    pub fn set_install_task_priority(&self, task_id: &str, priority: i64) -> Result<()> {
        let changed = self.connection()?.execute(
            "
            UPDATE install_tasks
            SET priority = ?2, updated_at_unix_seconds = ?3
            WHERE id = ?1 AND state IN ('queued', 'paused')
            ",
            params![task_id, priority, unix_timestamp()],
        )?;
        if changed == 0 {
            return Err(CoreError::InvalidInstallRequest(
                "任务不存在或当前状态不能调整优先级".to_owned(),
            ));
        }
        Ok(())
    }

    /// 共享执行槽的候选：按优先级与创建时间返回排队任务。
    pub fn queued_install_tasks_by_priority(&self) -> Result<Vec<String>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id FROM install_tasks
            WHERE state = 'queued'
            ORDER BY priority DESC, created_at_unix_seconds, id
            ",
        )?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn recover_interrupted_install_tasks(&self) -> Result<()> {
        self.connection()?.execute(
            "
            UPDATE install_tasks
            SET state = ?1, updated_at_unix_seconds = ?2
            WHERE state IN ('running', 'committing')
            ",
            params![
                TaskState::AwaitingRecovery.database_value(),
                unix_timestamp()
            ],
        )?;
        Ok(())
    }

    pub fn resolve_install_task_recovery(
        &self,
        task_id: &str,
        decision: RecoveryDecision,
    ) -> Result<()> {
        Uuid::parse_str(task_id)
            .map_err(|_| CoreError::InvalidInstallRequest("安装任务 ID 格式无效".to_owned()))?;
        let expected_staging = if decision == RecoveryDecision::Discard {
            Some(
                self.selected_data_directory()?
                    .join(".staging")
                    .join("install")
                    .join(task_id),
            )
        } else {
            None
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let task = transaction
            .query_row(
                "SELECT state, staging_directory FROM install_tasks WHERE id = ?1",
                params![task_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or_else(|| CoreError::InvalidInstallRequest("安装任务不存在".to_owned()))?;
        if TaskState::from_database(&task.0)? != TaskState::AwaitingRecovery {
            return Err(CoreError::InvalidInstallRequest(
                "任务当前不需要恢复确认".to_owned(),
            ));
        }

        if decision == RecoveryDecision::Discard {
            let expected_staging = expected_staging
                .as_deref()
                .ok_or_else(|| CoreError::InvalidStoredState("缺少受管任务暂存路径".to_owned()))?;
            if Path::new(&task.1) != expected_staging {
                return Err(CoreError::InvalidStoredState(
                    "任务暂存路径不在受管安装暂存区中，已拒绝清理".to_owned(),
                ));
            }
            if expected_staging.exists() {
                fs::remove_dir_all(expected_staging)?;
            }
        }

        let target_state = match decision {
            RecoveryDecision::Resume => TaskState::Queued,
            RecoveryDecision::Discard => TaskState::Cancelled,
        };
        let changed = transaction.execute(
            "
            UPDATE install_tasks
            SET state = ?2, updated_at_unix_seconds = ?3
            WHERE id = ?1 AND state = 'awaiting_recovery'
            ",
            params![task_id, target_state.database_value(), unix_timestamp()],
        )?;
        if changed == 0 {
            return Err(CoreError::InvalidInstallRequest(
                "任务恢复状态已经发生变化，请刷新后重试".to_owned(),
            ));
        }
        if decision == RecoveryDecision::Discard {
            transaction.execute(
                "
                DELETE FROM managed_java_environments
                WHERE status IN ('planned', 'failed')
                  AND id = (
                    SELECT environment_id FROM install_task_java WHERE task_id = ?1
                  )
                  AND NOT EXISTS (
                    SELECT 1
                    FROM install_task_java relation
                    JOIN install_tasks task ON task.id = relation.task_id
                    WHERE relation.environment_id = managed_java_environments.id
                      AND task.id <> ?1
                      AND task.state NOT IN ('cancelled', 'failed', 'completed')
                  )
                ",
                params![task_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn register_managed_java(
        &self,
        environment: &ManagedJavaEnvironment,
    ) -> Result<ManagedJavaEnvironment> {
        validate_java_identity(
            &environment.full_version,
            environment.distribution,
            environment.architecture,
        )?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = find_java_environment(
            &transaction,
            environment.distribution,
            &environment.full_version,
            environment.architecture,
        )? {
            transaction.execute(
                "
                UPDATE managed_java_environments
                SET home_directory = ?2, status = ?3
                WHERE id = ?1
                ",
                params![
                    existing.id,
                    environment.home_directory,
                    environment.status.database_value()
                ],
            )?;
            transaction.commit()?;
            return Ok(ManagedJavaEnvironment {
                home_directory: environment.home_directory.clone(),
                status: environment.status,
                ..existing
            });
        }
        insert_java_environment(&transaction, environment)?;
        transaction.commit()?;
        Ok(environment.clone())
    }

    pub fn list_managed_java(&self) -> Result<Vec<ManagedJavaEnvironment>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id, distribution, full_version, architecture, home_directory, status
            FROM managed_java_environments
            ORDER BY distribution, full_version, architecture
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        rows.map(|row| {
            let (id, distribution, full_version, architecture, home_directory, status) = row?;
            Ok(ManagedJavaEnvironment {
                id,
                distribution: JavaDistribution::from_database(&distribution)?,
                full_version,
                architecture: JavaArchitecture::from_database(&architecture)?,
                home_directory,
                status: JavaEnvironmentStatus::from_database(&status)?,
            })
        })
        .collect()
    }

    pub fn list_instances(&self) -> Result<Vec<ManagedInstanceSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id, name, game_version, loader_kind, loader_version, root_directory, state
            FROM instances
            WHERE NOT EXISTS (
                SELECT 1
                FROM recycle_bin_items
                WHERE recycle_bin_items.subject_id = instances.id
                  AND recycle_bin_items.item_kind = 'instance'
            )
            ORDER BY created_at_unix_seconds, id
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ManagedInstanceSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                game_version: row.get(2)?,
                loader_kind: row.get(3)?,
                loader_version: row.get(4)?,
                root_directory: row.get(5)?,
                state: row.get(6)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(CoreError::from)
    }

    pub fn install_task(&self, task_id: &str) -> Result<InstallTask> {
        self.list_install_tasks()?
            .into_iter()
            .find(|task| task.id == task_id)
            .ok_or_else(|| CoreError::InvalidInstallRequest("安装任务不存在".to_owned()))
    }

    pub(crate) fn set_task_phase(
        &self,
        task_id: &str,
        state: TaskState,
        stage: InstallStage,
        current_item: &str,
    ) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "
            UPDATE install_tasks
            SET state = ?2, current_stage = ?3, updated_at_unix_seconds = ?4
            WHERE id = ?1
            ",
            params![
                task_id,
                state.database_value(),
                stage.database_value(),
                unix_timestamp()
            ],
        )?;
        if changed == 0 {
            return Err(CoreError::InvalidInstallRequest(
                "安装任务不存在".to_owned(),
            ));
        }
        transaction.execute(
            "
            INSERT INTO task_progress (task_id, completed_bytes, current_item, error_summary)
            VALUES (?1, 0, ?2, NULL)
            ON CONFLICT(task_id) DO UPDATE SET
                current_item = excluded.current_item,
                error_summary = NULL
            ",
            params![task_id, current_item],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn set_task_progress(
        &self,
        task_id: &str,
        completed_bytes: u64,
        total_bytes: Option<u64>,
        current_item: Option<&str>,
    ) -> Result<()> {
        let completed = sqlite_integer(completed_bytes, "已完成字节数")?;
        let total = total_bytes
            .map(|value| sqlite_integer(value, "总字节数"))
            .transpose()?;
        self.connection()?.execute(
            "
            INSERT INTO task_progress (
                task_id, completed_bytes, total_bytes, current_item, error_summary
            ) VALUES (?1, ?2, ?3, ?4, NULL)
            ON CONFLICT(task_id) DO UPDATE SET
                completed_bytes = excluded.completed_bytes,
                total_bytes = excluded.total_bytes,
                current_item = excluded.current_item,
                error_summary = NULL
            ",
            params![task_id, completed, total, current_item],
        )?;
        Ok(())
    }

    pub(crate) fn set_task_source_detail(
        &self,
        task_id: &str,
        detail: &TaskSourceDetail,
    ) -> Result<()> {
        let serialized = serde_json::to_string(detail)?;
        self.connection()?.execute(
            "UPDATE task_progress SET source_detail = ?2 WHERE task_id = ?1",
            params![task_id, serialized],
        )?;
        Ok(())
    }

    pub(crate) fn mark_task_failed(&self, task_id: &str, error: &str) -> Result<()> {
        let summary: String = error.chars().take(4_000).collect();
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "
            UPDATE install_tasks
            SET state = 'failed', updated_at_unix_seconds = ?2
            WHERE id = ?1
            ",
            params![task_id, unix_timestamp()],
        )?;
        transaction.execute(
            "
            UPDATE task_progress
            SET current_item = '等待重试', error_summary = ?2
            WHERE task_id = ?1
            ",
            params![task_id, summary],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn mark_java_environment_status(
        &self,
        environment_id: &str,
        status: JavaEnvironmentStatus,
    ) -> Result<()> {
        let changed = self.connection()?.execute(
            "UPDATE managed_java_environments SET status = ?2 WHERE id = ?1",
            params![environment_id, status.database_value()],
        )?;
        if changed == 0 {
            return Err(CoreError::InvalidStoredState(
                "安装计划引用的 Java 环境不存在".to_owned(),
            ));
        }
        Ok(())
    }

    pub(crate) fn publish_ready_instance(
        &self,
        task: &InstallTask,
        java_environment_id: &str,
        runtime: &Value,
    ) -> Result<ManagedInstanceSummary> {
        let (loader_kind, loader_version) = match &task.plan.loader {
            ResolvedLoader::Vanilla => ("vanilla", None),
            ResolvedLoader::Fabric { version, .. } => ("fabric", Some(version.as_str())),
            ResolvedLoader::Quilt { version, .. } => ("quilt", Some(version.as_str())),
            ResolvedLoader::Forge { version, .. } => ("forge", Some(version.as_str())),
            ResolvedLoader::NeoForge { version, .. } => ("neoforge", Some(version.as_str())),
        };
        let instance = ManagedInstanceSummary {
            id: task.plan.instance_id.clone(),
            name: task.plan.instance_name.clone(),
            game_version: task.plan.game.version.id.clone(),
            loader_kind: loader_kind.to_owned(),
            loader_version: loader_version.map(str::to_owned),
            root_directory: task.target_directory.clone(),
            state: "ready".to_owned(),
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "
            INSERT INTO instances (
                id, name, game_version, loader_kind, loader_version,
                root_directory, state, created_at_unix_seconds
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'ready', ?7)
            ",
            params![
                instance.id,
                instance.name,
                instance.game_version,
                instance.loader_kind,
                instance.loader_version,
                instance.root_directory,
                unix_timestamp(),
            ],
        )?;
        transaction.execute(
            "
            INSERT INTO instance_runtime (
                instance_id, java_environment_id, plan_json, runtime_json
            ) VALUES (?1, ?2, ?3, ?4)
            ",
            params![
                instance.id,
                java_environment_id,
                serde_json::to_string(&task.plan)?,
                serde_json::to_string(runtime)?,
            ],
        )?;
        transaction.execute(
            "
            UPDATE install_tasks
            SET state = 'completed', current_stage = 'create_rollback_point',
                updated_at_unix_seconds = ?2
            WHERE id = ?1 AND state = 'committing'
            ",
            params![task.id, unix_timestamp()],
        )?;
        transaction.execute(
            "
            UPDATE task_progress
            SET completed_bytes = COALESCE(total_bytes, completed_bytes),
                current_item = '安装完成', error_summary = NULL
            WHERE task_id = ?1
            ",
            params![task.id],
        )?;
        transaction.commit()?;
        Ok(instance)
    }

    pub fn selected_data_directory(&self) -> Result<std::path::PathBuf> {
        let state = self.bootstrap_state()?;
        let selection: OnboardingSelection = state.settings.unwrap_or(state.defaults);
        Ok(std::path::PathBuf::from(selection.data_directory))
    }

    fn insert_install_task(
        &self,
        request: &ResolvedInstallRequest,
        task_id: &str,
        instance_id: &str,
        staging_directory: &Path,
        target_directory: &Path,
        shared_store_directory: &Path,
    ) -> Result<InstallTask> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let java_action = plan_java_action(&transaction, request, shared_store_directory)?;
        let stages = vec![
            InstallStage::Prepare,
            InstallStage::DownloadGameFiles,
            InstallStage::VerifyFiles,
            InstallStage::InstallGameEnvironment,
            InstallStage::ApplyLoader,
            InstallStage::CommitChanges,
            InstallStage::CreateRollbackPoint,
        ];
        let estimated_download_bytes = request
            .game
            .artifacts
            .iter()
            .fold(0_u64, |total, artifact| total.saturating_add(artifact.size))
            .saturating_add(request.game.asset_objects_total_bytes)
            .saturating_add(match &java_action {
                JavaPlanAction::Install { package, .. } => package.size,
                JavaPlanAction::Reuse { .. } | JavaPlanAction::AwaitExistingInstall { .. } => 0,
            });
        let target_text = path_text(target_directory);
        let plan = InstallPlan {
            schema_version: 1,
            instance_id: instance_id.to_owned(),
            instance_name: request.instance_name.trim().to_owned(),
            target_directory: target_text.clone(),
            shared_store_directory: path_text(shared_store_directory),
            game: request.game.clone(),
            loader: request.loader.clone(),
            java_action,
            isolation: request.isolation,
            stages,
            estimated_download_bytes,
        };
        let now = unix_timestamp();
        let progress_total = plan.estimated_download_bytes;
        let task = InstallTask {
            id: task_id.to_owned(),
            state: TaskState::Queued,
            current_stage: Some(InstallStage::Prepare),
            plan,
            staging_directory: path_text(staging_directory),
            target_directory: target_text,
            created_at_unix_seconds: now,
            updated_at_unix_seconds: now,
            priority: 0,
            paused_by: None,
            progress: TaskProgress {
                completed_bytes: 0,
                total_bytes: Some(progress_total),
                current_item: Some("等待执行".to_owned()),
                error_summary: None,
                source_detail: None,
            },
        };
        transaction.execute(
            "
            INSERT INTO install_tasks (
                id, state, current_stage, plan_json, staging_directory,
                target_directory, created_at_unix_seconds, updated_at_unix_seconds
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                task.id,
                task.state.database_value(),
                task.current_stage.map(InstallStage::database_value),
                serde_json::to_string(&task.plan)?,
                task.staging_directory,
                task.target_directory,
                task.created_at_unix_seconds,
                task.updated_at_unix_seconds,
            ],
        )?;
        let completed_db = sqlite_integer(task.progress.completed_bytes, "已完成字节数")?;
        let total_db = task
            .progress
            .total_bytes
            .map(|value| sqlite_integer(value, "总字节数"))
            .transpose()?;
        transaction.execute(
            "
            INSERT INTO task_progress (
                task_id, completed_bytes, total_bytes, current_item, error_summary
            ) VALUES (?1, ?2, ?3, ?4, NULL)
            ",
            params![task.id, completed_db, total_db, task.progress.current_item,],
        )?;
        let (environment_id, action) = match &task.plan.java_action {
            JavaPlanAction::Install { environment_id, .. } => (environment_id, "install"),
            JavaPlanAction::Reuse { environment_id, .. } => (environment_id, "reuse"),
            JavaPlanAction::AwaitExistingInstall { environment_id, .. } => {
                (environment_id, "await_existing_install")
            }
        };
        transaction.execute(
            "
            INSERT INTO install_task_java (task_id, environment_id, action)
            VALUES (?1, ?2, ?3)
            ",
            params![task.id, environment_id, action],
        )?;
        transaction.commit()?;
        Ok(task)
    }
}

fn plan_java_action(
    transaction: &Transaction<'_>,
    request: &ResolvedInstallRequest,
    shared_store_directory: &Path,
) -> Result<JavaPlanAction> {
    let package = &request.java;
    validate_java_identity(
        &package.full_version,
        package.distribution,
        package.architecture,
    )?;
    if let Some(environment) = find_java_environment(
        transaction,
        package.distribution,
        &package.full_version,
        package.architecture,
    )? {
        return Ok(match environment.status {
            JavaEnvironmentStatus::Ready => JavaPlanAction::Reuse {
                environment_id: environment.id,
                home_directory: environment.home_directory,
            },
            JavaEnvironmentStatus::Planned | JavaEnvironmentStatus::Installing => {
                JavaPlanAction::AwaitExistingInstall {
                    environment_id: environment.id,
                    target_directory: environment.home_directory,
                }
            }
            JavaEnvironmentStatus::Missing
            | JavaEnvironmentStatus::Failed
            | JavaEnvironmentStatus::Deleted => {
                // 显式新安装可重新下载同一身份;墓碑不会被静默复用,
                // 恢复已删环境属于用户在环境页主动发起的恢复,不走此路径。
                transaction.execute(
                    "UPDATE managed_java_environments SET status = 'planned' WHERE id = ?1",
                    params![environment.id],
                )?;
                JavaPlanAction::Install {
                    environment_id: environment.id,
                    target_directory: environment.home_directory,
                    package: package.artifact.clone(),
                }
            }
        });
    }

    let environment = ManagedJavaEnvironment {
        id: Uuid::new_v4().to_string(),
        distribution: package.distribution,
        full_version: package.full_version.clone(),
        architecture: package.architecture,
        home_directory: path_text(
            &shared_store_directory
                .join("java")
                .join(package.distribution.directory_name())
                .join(&package.full_version)
                .join(package.architecture.database_value()),
        ),
        status: JavaEnvironmentStatus::Planned,
    };
    insert_java_environment(transaction, &environment)?;
    Ok(JavaPlanAction::Install {
        environment_id: environment.id,
        target_directory: environment.home_directory,
        package: package.artifact.clone(),
    })
}

fn find_java_environment(
    transaction: &Transaction<'_>,
    distribution: JavaDistribution,
    full_version: &str,
    architecture: JavaArchitecture,
) -> Result<Option<ManagedJavaEnvironment>> {
    let stored = transaction
        .query_row(
            "
            SELECT id, distribution, full_version, architecture, home_directory, status
            FROM managed_java_environments
            WHERE distribution = ?1 AND full_version = ?2 AND architecture = ?3
            ",
            params![
                distribution.database_value(),
                full_version,
                architecture.database_value()
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    stored
        .map(
            |(id, distribution, full_version, architecture, home_directory, status)| {
                Ok(ManagedJavaEnvironment {
                    id,
                    distribution: JavaDistribution::from_database(&distribution)?,
                    full_version,
                    architecture: JavaArchitecture::from_database(&architecture)?,
                    home_directory,
                    status: JavaEnvironmentStatus::from_database(&status)?,
                })
            },
        )
        .transpose()
}

fn insert_java_environment(
    transaction: &Transaction<'_>,
    environment: &ManagedJavaEnvironment,
) -> Result<()> {
    transaction.execute(
        "
        INSERT INTO managed_java_environments (
            id, distribution, full_version, architecture, home_directory, status
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ",
        params![
            environment.id,
            environment.distribution.database_value(),
            environment.full_version,
            environment.architecture.database_value(),
            environment.home_directory,
            environment.status.database_value(),
        ],
    )?;
    Ok(())
}

fn validate_install_request(request: &ResolvedInstallRequest) -> Result<()> {
    let name = request.instance_name.trim();
    if name.is_empty() {
        return Err(CoreError::InvalidInstallRequest(
            "实例名称不能为空".to_owned(),
        ));
    }
    if name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(CoreError::InvalidInstallRequest(
            "实例名称不能超过 120 个字符或包含控制字符".to_owned(),
        ));
    }
    if request.game.version.id.trim().is_empty() || request.game.artifacts.is_empty() {
        return Err(CoreError::InvalidInstallRequest(
            "游戏版本或下载清单不完整".to_owned(),
        ));
    }
    if request
        .java
        .artifact
        .sha256
        .as_deref()
        .is_none_or(str::is_empty)
    {
        return Err(CoreError::InvalidInstallRequest(
            "托管 Java 包缺少 SHA-256，不能进入安装队列".to_owned(),
        ));
    }
    Ok(())
}

fn validate_java_identity(
    full_version: &str,
    _distribution: JavaDistribution,
    _architecture: JavaArchitecture,
) -> Result<()> {
    if full_version.trim().is_empty()
        || full_version
            .chars()
            .any(|character| character.is_control() || "\\/:*?\"<>|".contains(character))
    {
        return Err(CoreError::InvalidInstallRequest(
            "托管 Java 完整版本无效".to_owned(),
        ));
    }
    Ok(())
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn sqlite_integer(value: u64, label: &str) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| CoreError::InvalidStoredState(format!("{label}超过 SQLite 可表示范围")))
}

fn sqlite_unsigned(value: i64, label: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| CoreError::InvalidStoredState(format!("{label}不能是负数")))
}
