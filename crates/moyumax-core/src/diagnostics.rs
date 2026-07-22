use std::{
    collections::HashSet,
    env, fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use regex::{Captures, Regex};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::{
    AppService, CoreError, LaunchSessionState, LaunchSessionSummary, Result, unix_timestamp,
};

const REPORT_SCHEMA_VERSION: u32 = 1;
const MAX_EVIDENCE_BYTES: u64 = 512 * 1024;
const MAX_DISCOVERED_CRASH_FILES: usize = 8;
static CRASH_REPORT_CREATION_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CrashCauseKind {
    OutOfMemory,
    ModConflict,
    JavaRuntime,
    NativeCrash,
    LauncherInterrupted,
    Unknown,
}

impl CrashCauseKind {
    const fn title(self) -> &'static str {
        match self {
            Self::OutOfMemory => "游戏可用内存不足",
            Self::ModConflict => "可能存在模组或加载器冲突",
            Self::JavaRuntime => "Java 运行环境未能正常启动游戏",
            Self::NativeCrash => "游戏或图形驱动发生原生崩溃",
            Self::LauncherInterrupted => "启动器在游戏运行期间中断",
            Self::Unknown => "游戏异常退出，暂未识别明确原因",
        }
    }

    fn recommendations(self) -> Vec<String> {
        match self {
            Self::OutOfMemory => vec![
                "关闭占用大量内存的程序后再试；不要直接把全部物理内存分配给游戏。".to_owned(),
                "检查最近加入的高分辨率资源包、光影或大型模组。".to_owned(),
            ],
            Self::ModConflict => vec![
                "查看证据中的首个模组加载错误，并核对最近变更的模组及其依赖版本。".to_owned(),
                "MoyuMax 当前只提供建议，不会自动删除、禁用或安装模组。".to_owned(),
            ],
            Self::JavaRuntime => vec![
                "先运行实例完整性检查，确认托管 Java 与游戏版本匹配。".to_owned(),
                "不要手动替换 MoyuMax 托管目录中的 Java 文件。".to_owned(),
            ],
            Self::NativeCrash => vec![
                "更新或回退显卡驱动前先保留当前诊断包，并检查原生崩溃文本中的 Problematic frame。"
                    .to_owned(),
                "临时关闭光影或原生注入工具时应由用户明确操作。".to_owned(),
            ],
            Self::LauncherInterrupted => vec![
                "确认游戏进程是否仍在运行；若已经结束，可以再次启动实例。".to_owned(),
                "上次会话的最后输出可能不完整，结论仅作为边界说明。".to_owned(),
            ],
            Self::Unknown => vec![
                "展开证据查看最后输出，或预览并导出脱敏诊断包以便求助。".to_owned(),
                "在确认原因前不要批量删除模组、配置或存档。".to_owned(),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CrashEvidenceKind {
    GameOutput,
    GameLog,
    GameCrashReport,
    NativeCrash,
    LauncherLog,
    LaunchScript,
    Environment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashEvidenceItem {
    pub kind: CrashEvidenceKind,
    pub bundle_name: String,
    pub original_bytes: u64,
    pub included_bytes: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReportSummary {
    pub schema_version: u32,
    pub id: String,
    pub launch_session_id: String,
    pub instance_id: String,
    pub created_at_unix_seconds: i64,
    pub cause: CrashCauseKind,
    pub title: String,
    pub summary: String,
    pub recommendations: Vec<String>,
    pub evidence: Vec<CrashEvidenceItem>,
    pub redaction_summary: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticExportFile {
    pub bundle_name: String,
    pub included_bytes: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticExportPreview {
    pub report_id: String,
    pub suggested_file_name: String,
    pub files: Vec<DiagnosticExportFile>,
    pub total_bytes: u64,
    pub maximum_evidence_bytes: u64,
    pub redactions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticExportResult {
    pub report_id: String,
    pub archive_path: String,
    pub archive_bytes: u64,
    pub file_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredEvidenceItem {
    #[serde(flatten)]
    public: CrashEvidenceItem,
    source_path: String,
}

#[derive(Debug)]
struct StoredReport {
    public: CrashReportSummary,
    evidence: Vec<StoredEvidenceItem>,
    report_directory: PathBuf,
}

#[derive(Debug)]
struct SessionContext {
    session: LaunchSessionSummary,
    instance_root: PathBuf,
    working_directory: PathBuf,
    game_version: String,
    loader_kind: String,
    loader_version: Option<String>,
}

#[derive(Debug)]
struct SourceText {
    text: String,
    original_bytes: u64,
    truncated: bool,
}

#[derive(Debug, Clone)]
struct DiscoveredCrashFile {
    path: PathBuf,
    kind: CrashEvidenceKind,
}

impl AppService {
    pub fn create_crash_report_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<CrashReportSummary>> {
        let _creation_guard = CRASH_REPORT_CREATION_LOCK.lock().map_err(|_| {
            CoreError::Diagnostics("崩溃报告生成锁已损坏，请重启 MoyuMax".to_owned())
        })?;
        if let Some(existing) = self.crash_report_for_session(session_id)? {
            return Ok(Some(existing));
        }
        let Some(context) = self.diagnostic_session_context(session_id)? else {
            return Err(CoreError::Diagnostics(
                "启动会话不存在，无法生成崩溃报告".to_owned(),
            ));
        };
        if !matches!(
            context.session.state,
            LaunchSessionState::Failed | LaunchSessionState::Interrupted
        ) {
            return Ok(None);
        }

        let report_id = format!("crash-{}", context.session.id);
        let reports_root = self.selected_data_directory()?.join("diagnostics/reports");
        let report_directory = reports_root.join(&report_id);
        let staging_directory =
            reports_root.join(format!(".staging-{report_id}-{}", Uuid::new_v4()));
        fs::create_dir_all(&reports_root).map_err(|error| {
            CoreError::Diagnostics(format!(
                "无法创建崩溃报告目录 {}：{error}",
                reports_root.display()
            ))
        })?;
        if staging_directory.exists() {
            fs::remove_dir_all(&staging_directory)?;
        }
        fs::create_dir_all(&staging_directory)?;

        let result =
            self.build_crash_report(&context, &report_id, &staging_directory, &report_directory);
        let (report, evidence) = match result {
            Ok(value) => value,
            Err(error) => {
                let _ = fs::remove_dir_all(&staging_directory);
                return Err(error);
            }
        };
        let publish = (|| -> Result<()> {
            let report_json = serde_json::to_string_pretty(&report)?;
            fs::write(
                staging_directory.join("report.json"),
                report_json.as_bytes(),
            )?;
            sync_tree_files(&staging_directory)?;
            if report_directory.exists() {
                fs::remove_dir_all(&report_directory)?;
            }
            fs::rename(&staging_directory, &report_directory).map_err(|error| {
                CoreError::Diagnostics(format!(
                    "无法原子发布崩溃报告 {}：{error}",
                    report_directory.display()
                ))
            })?;
            Ok(())
        })();
        if let Err(error) = publish {
            let _ = fs::remove_dir_all(&staging_directory);
            return Err(error);
        }

        let stored_json = serde_json::to_string(&evidence)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let insert = transaction.execute(
            "INSERT INTO crash_reports (id, launch_session_id, instance_id, created_at_unix_seconds, report_json, evidence_json, report_directory) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                report.id,
                report.launch_session_id,
                report.instance_id,
                report.created_at_unix_seconds,
                serde_json::to_string(&report)?,
                stored_json,
                path_text(&report_directory)?,
            ],
        );
        if let Err(error) = insert.and_then(|_| transaction.commit()) {
            let _ = fs::remove_dir_all(&report_directory);
            if let Some(existing) = self.crash_report_for_session(session_id)? {
                return Ok(Some(existing));
            }
            return Err(error.into());
        }
        Ok(Some(report))
    }

    pub fn list_crash_reports(&self) -> Result<Vec<CrashReportSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT report_json FROM crash_reports ORDER BY created_at_unix_seconds DESC, id DESC",
        )?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|row| Ok(serde_json::from_str(&row?)?))
            .collect()
    }

    pub fn get_crash_report(&self, report_id: &str) -> Result<CrashReportSummary> {
        self.stored_crash_report(report_id)
            .map(|stored| stored.public)
    }

    pub fn preview_diagnostic_export(&self, report_id: &str) -> Result<DiagnosticExportPreview> {
        let stored = self.stored_crash_report(report_id)?;
        let mut files = Vec::with_capacity(stored.evidence.len() + 2);
        let report_path = stored.report_directory.join("report.json");
        files.push(DiagnosticExportFile {
            bundle_name: "report.json".to_owned(),
            included_bytes: regular_file_size(&report_path).unwrap_or(0),
            truncated: false,
        });
        files.extend(stored.evidence.iter().map(|item| DiagnosticExportFile {
            bundle_name: item.public.bundle_name.clone(),
            included_bytes: item.public.included_bytes,
            truncated: item.public.truncated,
        }));
        let manifest_bytes = u64::try_from(
            serde_json::to_vec(&json!({
                "schemaVersion": REPORT_SCHEMA_VERSION,
                "reportId": report_id,
                "redactions": stored.public.redaction_summary,
            }))?
            .len(),
        )
        .unwrap_or(u64::MAX);
        files.push(DiagnosticExportFile {
            bundle_name: "manifest.json".to_owned(),
            included_bytes: manifest_bytes,
            truncated: false,
        });
        files.sort_by(|left, right| left.bundle_name.cmp(&right.bundle_name));
        let total_bytes = files.iter().fold(0_u64, |total, file| {
            total.saturating_add(file.included_bytes)
        });
        Ok(DiagnosticExportPreview {
            report_id: report_id.to_owned(),
            suggested_file_name: format!("MoyuMax-diagnostics-{report_id}.zip"),
            files,
            total_bytes,
            maximum_evidence_bytes: MAX_EVIDENCE_BYTES,
            redactions: stored.public.redaction_summary,
        })
    }

    pub fn export_diagnostic_bundle(&self, report_id: &str) -> Result<DiagnosticExportResult> {
        let stored = self.stored_crash_report(report_id)?;
        let context = self
            .diagnostic_session_context(&stored.public.launch_session_id)?
            .ok_or_else(|| CoreError::Diagnostics("诊断包引用的启动会话已不存在".to_owned()))?;
        let redactor = DiagnosticRedactor::new(&context);
        let preview = self.preview_diagnostic_export(report_id)?;
        let exports_directory = self.selected_data_directory()?.join("diagnostics/exports");
        fs::create_dir_all(&exports_directory)?;
        let unique = Uuid::new_v4();
        let archive_path =
            exports_directory.join(format!("MoyuMax-diagnostics-{report_id}-{unique}.zip"));
        let partial_path = archive_path.with_extension("zip.partial");
        let export_result = write_diagnostic_zip(&partial_path, &stored, &preview, &redactor);
        let file = match export_result {
            Ok(file) => file,
            Err(error) => {
                let _ = fs::remove_file(&partial_path);
                return Err(error);
            }
        };
        file.sync_all()?;
        drop(file);
        fs::rename(&partial_path, &archive_path).map_err(|error| {
            let _ = fs::remove_file(&partial_path);
            CoreError::Diagnostics(format!(
                "无法原子发布诊断包 {}：{error}",
                archive_path.display()
            ))
        })?;
        let archive_bytes = fs::metadata(&archive_path)?.len();
        Ok(DiagnosticExportResult {
            report_id: report_id.to_owned(),
            archive_path: path_text(&archive_path)?,
            archive_bytes,
            file_count: preview.files.len(),
        })
    }

    pub(crate) fn generate_missing_crash_reports(&self) -> Result<()> {
        let session_ids = {
            let connection = self.connection()?;
            let mut statement = connection.prepare(
                "SELECT id FROM launch_sessions WHERE state IN ('failed', 'interrupted') AND NOT EXISTS (SELECT 1 FROM crash_reports WHERE crash_reports.launch_session_id = launch_sessions.id) ORDER BY started_at_unix_seconds",
            )?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        for session_id in session_ids {
            let _ = self.create_crash_report_for_session(&session_id);
        }
        Ok(())
    }

    fn build_crash_report(
        &self,
        context: &SessionContext,
        report_id: &str,
        staging_directory: &Path,
        report_directory: &Path,
    ) -> Result<(CrashReportSummary, Vec<StoredEvidenceItem>)> {
        let redactor = DiagnosticRedactor::new(context);
        let evidence_writer = EvidenceWriter {
            staging_directory,
            report_directory,
            redactor: &redactor,
        };
        let mut stored = Vec::new();
        let mut analysis_text = String::new();

        let stdout = read_text_tail(Path::new(&context.session.stdout_path))?;
        let stderr = read_text_tail(Path::new(&context.session.stderr_path))?;
        if stdout.is_some() || stderr.is_some() {
            let mut combined = String::new();
            if let Some(source) = &stdout {
                combined.push_str("[stdout]\n");
                combined.push_str(&source.text);
                combined.push('\n');
            }
            if let Some(source) = &stderr {
                combined.push_str("[stderr]\n");
                combined.push_str(&source.text);
            }
            analysis_text.push_str(&combined);
            let original_bytes = stdout
                .as_ref()
                .map_or(0, |value| value.original_bytes)
                .saturating_add(stderr.as_ref().map_or(0, |value| value.original_bytes));
            let truncated = stdout.as_ref().is_some_and(|value| value.truncated)
                || stderr.as_ref().is_some_and(|value| value.truncated);
            let combined_source = SourceText {
                text: combined,
                original_bytes,
                truncated,
            };
            stored.push(evidence_writer.write(
                CrashEvidenceKind::GameOutput,
                "game/last-output.log",
                &combined_source,
            )?);
        }

        for (relative, kind, bundle) in [
            (
                "logs/latest.log",
                CrashEvidenceKind::GameLog,
                "game/latest.log",
            ),
            (
                "logs/debug.log",
                CrashEvidenceKind::GameLog,
                "game/debug.log",
            ),
        ] {
            if let Some(source) = read_text_tail(&context.working_directory.join(relative))? {
                analysis_text.push_str(&source.text);
                analysis_text.push('\n');
                stored.push(evidence_writer.write(kind, bundle, &source)?);
            }
        }

        let session_logs = Path::new(&context.session.stdout_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| context.working_directory.join("logs/moyumax"));
        for (suffix, kind, bundle) in [
            (
                "launcher.log",
                CrashEvidenceKind::LauncherLog,
                "moyumax/launcher.log",
            ),
            (
                "launch-redacted.cmd.txt",
                CrashEvidenceKind::LaunchScript,
                "moyumax/launch-redacted.cmd.txt",
            ),
        ] {
            let path = session_logs.join(format!("{}.{suffix}", context.session.id));
            if let Some(source) = read_text_tail(&path)? {
                analysis_text.push_str(&source.text);
                analysis_text.push('\n');
                stored.push(evidence_writer.write(kind, bundle, &source)?);
            }
        }

        let mut discovered = discover_crash_text_files(context)?;
        discovered.sort_by(|left, right| left.path.cmp(&right.path));
        discovered.truncate(MAX_DISCOVERED_CRASH_FILES);
        for (index, discovered) in discovered.iter().enumerate() {
            if let Some(source) = read_text_tail(&discovered.path)? {
                analysis_text.push_str(&source.text);
                analysis_text.push('\n');
                let name = discovered
                    .path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(safe_bundle_component)
                    .unwrap_or_else(|| format!("crash-{index}.txt"));
                stored.push(evidence_writer.write(
                    discovered.kind,
                    &format!("game/crash-reports/{index}-{name}"),
                    &source,
                )?);
            }
        }

        let environment = serde_json::to_string_pretty(&json!({
            "schemaVersion": REPORT_SCHEMA_VERSION,
            "operatingSystem": env::consts::OS,
            "architecture": env::consts::ARCH,
            "gameVersion": context.game_version,
            "loaderKind": context.loader_kind,
            "loaderVersion": context.loader_version,
            "sessionState": context.session.state,
            "exitCode": context.session.exit_code,
        }))?;
        let environment_source = SourceText {
            original_bytes: u64::try_from(environment.len()).unwrap_or(u64::MAX),
            text: environment,
            truncated: false,
        };
        stored.push(evidence_writer.write(
            CrashEvidenceKind::Environment,
            "moyumax/environment.json",
            &environment_source,
        )?);

        let cause = analyze_cause(context.session.state, &analysis_text);
        let summary = diagnostic_summary(cause, context.session.exit_code);
        let evidence = stored.iter().map(|item| item.public.clone()).collect();
        let report = CrashReportSummary {
            schema_version: REPORT_SCHEMA_VERSION,
            id: report_id.to_owned(),
            launch_session_id: context.session.id.clone(),
            instance_id: context.session.instance_id.clone(),
            created_at_unix_seconds: unix_timestamp(),
            cause,
            title: cause.title().to_owned(),
            summary,
            recommendations: cause.recommendations(),
            evidence,
            redaction_summary: DiagnosticRedactor::redaction_summary(),
        };
        Ok((report, stored))
    }

    fn diagnostic_session_context(&self, session_id: &str) -> Result<Option<SessionContext>> {
        let session = self
            .list_launch_sessions()?
            .into_iter()
            .find(|session| session.id == session_id);
        let Some(session) = session else {
            return Ok(None);
        };
        let connection = self.connection()?;
        let stored = connection
            .query_row(
                "SELECT i.root_directory, i.game_version, i.loader_kind, i.loader_version, r.runtime_json FROM instances i LEFT JOIN instance_runtime r ON r.instance_id = i.id WHERE i.id = ?1",
                params![session.instance_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| CoreError::Diagnostics("崩溃会话引用的实例不存在".to_owned()))?;
        let instance_root = PathBuf::from(&stored.0);
        let working_directory = stored
            .4
            .as_deref()
            .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
            .and_then(|value| {
                value
                    .get("workingDirectory")
                    .and_then(serde_json::Value::as_str)
                    .map(PathBuf::from)
            })
            .unwrap_or_else(|| instance_root.join(".minecraft"));
        Ok(Some(SessionContext {
            session,
            instance_root,
            working_directory,
            game_version: stored.1,
            loader_kind: stored.2,
            loader_version: stored.3,
        }))
    }

    fn crash_report_for_session(&self, session_id: &str) -> Result<Option<CrashReportSummary>> {
        self.connection()?
            .query_row(
                "SELECT report_json FROM crash_reports WHERE launch_session_id = ?1",
                params![session_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|value| serde_json::from_str(&value).map_err(CoreError::from))
            .transpose()
    }

    fn stored_crash_report(&self, report_id: &str) -> Result<StoredReport> {
        let connection = self.connection()?;
        let stored = connection
            .query_row(
                "SELECT report_json, evidence_json, report_directory FROM crash_reports WHERE id = ?1",
                params![report_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| CoreError::Diagnostics("崩溃报告不存在".to_owned()))?;
        Ok(StoredReport {
            public: serde_json::from_str(&stored.0)?,
            evidence: serde_json::from_str(&stored.1)?,
            report_directory: PathBuf::from(stored.2),
        })
    }
}

#[derive(Debug)]
struct DiagnosticRedactor {
    literal_replacements: Vec<(String, &'static str)>,
}

impl DiagnosticRedactor {
    fn new(context: &SessionContext) -> Self {
        Self::from_literals(vec![
            (context.session.player_name.clone(), "<redacted-player>"),
            (
                context.instance_root.to_string_lossy().into_owned(),
                "<redacted-instance-directory>",
            ),
            (
                context.working_directory.to_string_lossy().into_owned(),
                "<redacted-instance-directory>",
            ),
        ])
    }

    fn from_literals(mut replacements: Vec<(String, &'static str)>) -> Self {
        for key in ["USERPROFILE", "HOME"] {
            if let Some(value) = env::var_os(key) {
                let value = value.to_string_lossy().into_owned();
                if !value.is_empty() {
                    replacements.push((value, "<redacted-user-directory>"));
                }
            }
        }
        if let Ok(value) = env::var("USERNAME")
            && !value.is_empty()
        {
            replacements.push((value, "<redacted-user>"));
        }
        replacements.sort_by_key(|item| std::cmp::Reverse(item.0.len()));
        replacements.dedup_by(|left, right| left.0 == right.0);
        Self {
            literal_replacements: replacements,
        }
    }

    fn sanitize(&self, value: &str) -> Result<String> {
        let mut sanitized = value.to_owned();
        for (literal, replacement) in &self.literal_replacements {
            if !literal.is_empty() {
                sanitized = sanitized.replace(literal, replacement);
                sanitized = sanitized.replace(&literal.replace('\\', "/"), replacement);
            }
        }
        sanitized = replace_regex(
            &sanitized,
            r#"(?i)(access[_-]?token|client[_-]?token|refresh[_-]?token|password|passwd|authorization)\s*[:=]\s*(?:Bearer\s+)?[^\s"']+"#,
            |captures| format!("{}=<redacted-credential>", &captures[1]),
        )?;
        sanitized = replace_regex(
            &sanitized,
            r#"(?i)(--?(?:accessToken|clientToken|refreshToken|password|authorization|username|uuid|clientId|xuid))\s+[^\s"']+"#,
            |captures| format!("{} <redacted-account>", &captures[1]),
        )?;
        sanitized = replace_regex(&sanitized, r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]+", |_| {
            "Bearer <redacted-credential>".to_owned()
        })?;
        sanitized = replace_regex(
            &sanitized,
            r"(?i)((?:server|address|ip)\s*[:=]\s*)[^\s,;]+",
            |captures| format!("{}<redacted-server>", &captures[1]),
        )?;
        sanitized = replace_regex(&sanitized, r"\b(?:\d{1,3}\.){3}\d{1,3}\b", |_| {
            "<redacted-server>".to_owned()
        })?;
        sanitized = replace_regex(
            &sanitized,
            r"(?i)\b[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?:\d{2,5}\b",
            |_| "<redacted-server>".to_owned(),
        )?;
        sanitized = replace_regex(
            &sanitized,
            r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b",
            |_| "<redacted-account-id>".to_owned(),
        )?;
        sanitized = replace_regex(&sanitized, r"(?i)[A-Z]:\\Users\\[^\\\r\n]+", |_| {
            "<redacted-user-directory>".to_owned()
        })?;
        sanitized = replace_regex(&sanitized, r"/(?:home|Users)/[^/\r\n]+", |_| {
            "<redacted-user-directory>".to_owned()
        })?;
        Ok(sanitized)
    }

    fn redaction_summary() -> Vec<String> {
        vec![
            "玩家名称与账户标识替换为占位符。".to_owned(),
            "用户目录和实例绝对路径替换为占位符。".to_owned(),
            "IP、域名端口和显式服务器地址替换为占位符。".to_owned(),
            "令牌、密码、Authorization 和类似凭据字段替换为占位符。".to_owned(),
            format!(
                "每个文本证据最多保留最后 {} KiB。",
                MAX_EVIDENCE_BYTES / 1024
            ),
        ]
    }
}

pub(crate) fn write_launch_diagnostic_files(
    logs_directory: &Path,
    session_id: &str,
    raw_script: &str,
    sensitive_values: &[String],
) -> Result<()> {
    fs::create_dir_all(logs_directory)?;
    let replacements = sensitive_values
        .iter()
        .filter(|value| !value.is_empty())
        .cloned()
        .map(|value| (value, "<redacted-local-value>"))
        .collect();
    let redactor = DiagnosticRedactor::from_literals(replacements);
    let script = redactor.sanitize(raw_script)?;
    let script_path = logs_directory.join(format!("{session_id}.launch-redacted.cmd.txt"));
    write_atomic_text(&script_path, &script)?;
    let launcher_log_path = logs_directory.join(format!("{session_id}.launcher.log"));
    if let Err(error) = write_atomic_text(
        &launcher_log_path,
        &format!(
            "{}\t启动完整性检查通过，已生成脱敏启动脚本\n",
            unix_timestamp()
        ),
    ) {
        let _ = fs::remove_file(script_path);
        return Err(error);
    }
    Ok(())
}

pub(crate) fn append_launch_diagnostic_event(
    session: &LaunchSessionSummary,
    event: &str,
) -> Result<()> {
    let parent = Path::new(&session.stdout_path)
        .parent()
        .ok_or_else(|| CoreError::Diagnostics("启动会话日志目录不可用".to_owned()))?;
    fs::create_dir_all(parent)?;
    let path = parent.join(format!("{}.launcher.log", session.id));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{}\t{event}", unix_timestamp())?;
    file.sync_data()?;
    Ok(())
}

pub(crate) fn remove_launch_diagnostic_files(logs_directory: &Path, session_id: &str) {
    for suffix in ["launch-redacted.cmd.txt", "launcher.log"] {
        let path = logs_directory.join(format!("{session_id}.{suffix}"));
        // 新会话使用随机 ID；持久化失败时只补偿删除本次生成的两个诊断文件。
        let _ = fs::remove_file(path);
    }
}

fn write_atomic_text(path: &Path, value: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::Diagnostics("诊断文件缺少父目录".to_owned()))?;
    fs::create_dir_all(parent)?;
    let partial = parent.join(format!(
        ".{}.{}.partial",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("diagnostic"),
        Uuid::new_v4()
    ));
    let write_result = (|| -> Result<()> {
        let mut file = fs::File::create(&partial)?;
        file.write_all(value.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&partial, path)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    write_result
}

fn replace_regex<F>(value: &str, pattern: &str, replacement: F) -> Result<String>
where
    F: Fn(&Captures<'_>) -> String,
{
    let regex = Regex::new(pattern)
        .map_err(|error| CoreError::Diagnostics(format!("诊断脱敏规则无效：{error}")))?;
    Ok(regex.replace_all(value, replacement).into_owned())
}

struct EvidenceWriter<'a> {
    staging_directory: &'a Path,
    report_directory: &'a Path,
    redactor: &'a DiagnosticRedactor,
}

impl EvidenceWriter<'_> {
    fn write(
        &self,
        kind: CrashEvidenceKind,
        bundle_name: &str,
        source: &SourceText,
    ) -> Result<StoredEvidenceItem> {
        if !safe_bundle_name(bundle_name) {
            return Err(CoreError::Diagnostics(format!(
                "诊断证据名称不安全：{bundle_name}"
            )));
        }
        let sanitized = self.redactor.sanitize(&source.text)?;
        let staging_path = self.staging_directory.join(bundle_name);
        if let Some(parent) = staging_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&staging_path, sanitized.as_bytes())?;
        let included_bytes = u64::try_from(sanitized.len()).unwrap_or(u64::MAX);
        Ok(StoredEvidenceItem {
            public: CrashEvidenceItem {
                kind,
                bundle_name: bundle_name.to_owned(),
                original_bytes: source.original_bytes,
                included_bytes,
                truncated: source.truncated,
            },
            source_path: path_text(&self.report_directory.join(bundle_name))?,
        })
    }
}

fn read_text_tail(path: &Path) -> Result<Option<SourceText>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if unavailable_evidence(&error) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() {
        return Ok(None);
    }
    let original_bytes = metadata.len();
    let truncated = original_bytes > MAX_EVIDENCE_BYTES;
    let start = original_bytes.saturating_sub(MAX_EVIDENCE_BYTES);
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if unavailable_evidence(&error) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    file.seek(SeekFrom::Start(start))?;
    let mut bytes =
        Vec::with_capacity(usize::try_from(original_bytes.min(MAX_EVIDENCE_BYTES)).unwrap_or(0));
    file.read_to_end(&mut bytes)?;
    Ok(Some(SourceText {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        original_bytes,
        truncated,
    }))
}

fn discover_crash_text_files(context: &SessionContext) -> Result<Vec<DiscoveredCrashFile>> {
    let mut paths = Vec::new();
    for directory in [
        context.working_directory.join("crash-reports"),
        context.instance_root.join("crash-reports"),
    ] {
        collect_text_files(&directory, CrashEvidenceKind::GameCrashReport, &mut paths)?;
    }
    for directory in [&context.working_directory, &context.instance_root] {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if unavailable_evidence(&error) => continue,
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if (name.starts_with("hs_err_pid") || name.starts_with("crash-"))
                && is_regular_text_file(&entry.path())?
            {
                paths.push(DiscoveredCrashFile {
                    path: entry.path(),
                    kind: CrashEvidenceKind::NativeCrash,
                });
            }
        }
    }
    let mut seen = HashSet::new();
    paths.retain(|item| seen.insert(item.path.clone()));
    Ok(paths)
}

fn collect_text_files(
    directory: &Path,
    kind: CrashEvidenceKind,
    paths: &mut Vec<DiscoveredCrashFile>,
) -> Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if unavailable_evidence(&error) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        if is_regular_text_file(&entry.path())? {
            paths.push(DiscoveredCrashFile {
                path: entry.path(),
                kind,
            });
        }
    }
    Ok(())
}

fn is_regular_text_file(path: &Path) -> Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    Ok(metadata.file_type().is_file() && matches!(extension.as_str(), "log" | "txt" | "json"))
}

fn analyze_cause(state: LaunchSessionState, evidence: &str) -> CrashCauseKind {
    if state == LaunchSessionState::Interrupted {
        return CrashCauseKind::LauncherInterrupted;
    }
    let value = evidence.to_ascii_lowercase();
    if value.contains("outofmemoryerror") || value.contains("java heap space") {
        CrashCauseKind::OutOfMemory
    } else if [
        "modresolutionexception",
        "incompatible mod set",
        "mixin apply failed",
        "mod conflict",
    ]
    .iter()
    .any(|needle| value.contains(needle))
    {
        CrashCauseKind::ModConflict
    } else if [
        "exception_access_violation",
        "problematic frame",
        "hs_err_pid",
        "native crash",
    ]
    .iter()
    .any(|needle| value.contains(needle))
    {
        CrashCauseKind::NativeCrash
    } else if [
        "unsupportedclassversionerror",
        "could not create the java virtual machine",
        "jni error has occurred",
    ]
    .iter()
    .any(|needle| value.contains(needle))
    {
        CrashCauseKind::JavaRuntime
    } else {
        CrashCauseKind::Unknown
    }
}

fn diagnostic_summary(cause: CrashCauseKind, exit_code: Option<i32>) -> String {
    let exit = exit_code.map_or_else(
        || "游戏进程没有返回退出码".to_owned(),
        |code| format!("游戏进程返回退出码 {code}"),
    );
    match cause {
        CrashCauseKind::OutOfMemory => {
            format!("{exit}。最后输出包含 Java 内存不足证据；这不等于存档已经损坏。")
        }
        CrashCauseKind::ModConflict => {
            format!("{exit}。日志包含模组解析或 Mixin 应用失败的证据，需要核对具体版本。")
        }
        CrashCauseKind::JavaRuntime => {
            format!("{exit}。日志表明 Java 虚拟机或字节码版本未能正常启动。")
        }
        CrashCauseKind::NativeCrash => {
            format!("{exit}。发现原生崩溃特征，可能涉及图形驱动、JNI 或本地库。")
        }
        CrashCauseKind::LauncherInterrupted => {
            "MoyuMax 重启时发现该游戏会话仍标记为运行，因此将其记录为中断；最后输出可能不完整。"
                .to_owned()
        }
        CrashCauseKind::Unknown => {
            format!("{exit}，现有本地规则没有识别出唯一原因。请查看证据后再决定下一步。")
        }
    }
}

fn write_diagnostic_zip(
    partial_path: &Path,
    stored: &StoredReport,
    preview: &DiagnosticExportPreview,
    redactor: &DiagnosticRedactor,
) -> Result<fs::File> {
    let file = fs::File::create(partial_path)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600);
    let manifest = serde_json::to_string_pretty(&json!({
        "schemaVersion": REPORT_SCHEMA_VERSION,
        "report": stored.public,
        "files": preview.files,
        "redactions": preview.redactions,
        "uploadPerformed": false,
    }))?;
    writer
        .start_file("manifest.json", options)
        .map_err(zip_error)?;
    writer.write_all(manifest.as_bytes())?;

    let report_json = fs::read_to_string(stored.report_directory.join("report.json"))?;
    writer
        .start_file("report.json", options)
        .map_err(zip_error)?;
    writer.write_all(report_json.as_bytes())?;

    for evidence in &stored.evidence {
        if !safe_bundle_name(&evidence.public.bundle_name) {
            return Err(CoreError::Diagnostics(format!(
                "诊断证据名称不安全：{}",
                evidence.public.bundle_name
            )));
        }
        let source = Path::new(&evidence.source_path);
        let text = read_text_tail(source)?.ok_or_else(|| {
            CoreError::Diagnostics(format!("诊断证据已不可用：{}", evidence.public.bundle_name))
        })?;
        writer
            .start_file(&evidence.public.bundle_name, options)
            .map_err(zip_error)?;
        let sanitized = redactor.sanitize(&text.text)?;
        writer.write_all(sanitized.as_bytes())?;
    }
    writer.finish().map_err(zip_error)
}

fn zip_error(error: zip::result::ZipError) -> CoreError {
    CoreError::Diagnostics(format!("无法写入诊断 ZIP：{error}"))
}

fn sync_tree_files(root: &Path) -> Result<()> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                stack.push(entry.path());
            } else if metadata.is_file() {
                fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(entry.path())?
                    .sync_all()?;
            }
        }
    }
    Ok(())
}

fn regular_file_size(path: &Path) -> Option<u64> {
    fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.file_type().is_file())
        .map(|metadata| metadata.len())
}

fn unavailable_evidence(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound
            | std::io::ErrorKind::PermissionDenied
            | std::io::ErrorKind::NotADirectory
    )
}

fn safe_bundle_name(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && !path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
        && !value.contains("..")
        && !value.contains('\\')
}

fn safe_bundle_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn path_text(path: &Path) -> Result<String> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        CoreError::Diagnostics(format!("诊断路径不是有效 Unicode：{}", path.display()))
    })
}
