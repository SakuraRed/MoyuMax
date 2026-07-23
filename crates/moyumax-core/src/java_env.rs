//! 托管 Java 环境管理：清单、删除墓碑、恢复与实例指派。
//!
//! 删除采用墓碑语义：环境文件移除但身份与引用保留，
//! 实例启动时环境缺失必须明确报错并指引恢复，
//! 恢复由用户主动触发并解析同线最新可用补丁。

use std::{fs, path::Path};

use rusqlite::{TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::{
    AppService, ArtifactDownloader, CoreError, JavaArchitecture, JavaDistribution,
    JavaEnvironmentStatus, MetadataClient, Result, SourcePolicy,
};

/// 引用某环境的实例。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencingInstanceSummary {
    pub id: String,
    pub name: String,
}

/// 环境清单条目：身份、位置、大小、健康与引用实例。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaEnvironmentSummary {
    pub id: String,
    pub distribution: JavaDistribution,
    pub full_version: String,
    pub architecture: JavaArchitecture,
    pub home_directory: String,
    pub status: JavaEnvironmentStatus,
    pub size_bytes: u64,
    pub healthy: bool,
    pub referencing_instances: Vec<ReferencingInstanceSummary>,
}

/// 删除结果：已删除，或需要用户确认受影响实例。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JavaDeleteOutcome {
    #[serde(rename_all = "camelCase")]
    Deleted { files_removed: bool },
    #[serde(rename_all = "camelCase")]
    RequiresConfirmation {
        instances: Vec<ReferencingInstanceSummary>,
    },
}

impl AppService {
    /// 全部未删除的托管 Java 环境（含大小、健康与引用实例）。
    pub fn list_java_environments(&self) -> Result<Vec<JavaEnvironmentSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id, distribution, full_version, architecture, home_directory, status
            FROM managed_java_environments
            WHERE status <> 'deleted'
            ORDER BY distribution, full_version, architecture
            ",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut environments = Vec::with_capacity(rows.len());
        for (id, distribution, full_version, architecture, home_directory, status) in rows {
            let references = self.referencing_instances(&id)?;
            let size_bytes = directory_size(Path::new(&home_directory));
            let healthy = Path::new(&home_directory).join("bin/java.exe").is_file();
            environments.push(JavaEnvironmentSummary {
                id,
                distribution: JavaDistribution::from_database(&distribution)?,
                full_version,
                architecture: JavaArchitecture::from_database(&architecture)?,
                home_directory,
                status: JavaEnvironmentStatus::from_database(&status)?,
                size_bytes,
                healthy,
                referencing_instances: references,
            });
        }
        Ok(environments)
    }

    /// 已删除（墓碑）环境，用于恢复入口。
    pub fn list_deleted_java_environments(&self) -> Result<Vec<JavaEnvironmentSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id, distribution, full_version, architecture, home_directory, status
            FROM managed_java_environments
            WHERE status = 'deleted'
            ORDER BY distribution, full_version, architecture
            ",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut environments = Vec::with_capacity(rows.len());
        for (id, distribution, full_version, architecture, home_directory, status) in rows {
            let references = self.referencing_instances(&id)?;
            environments.push(JavaEnvironmentSummary {
                id,
                distribution: JavaDistribution::from_database(&distribution)?,
                full_version,
                architecture: JavaArchitecture::from_database(&architecture)?,
                home_directory,
                status: JavaEnvironmentStatus::from_database(&status)?,
                size_bytes: 0,
                healthy: false,
                referencing_instances: references,
            });
        }
        Ok(environments)
    }

    /// 删除环境：有引用时必须确认；只清理受管 Java 根内的文件，墓碑保留。
    pub fn delete_java_environment(
        &self,
        environment_id: &str,
        force: bool,
    ) -> Result<JavaDeleteOutcome> {
        let references = self.referencing_instances(environment_id)?;
        if !references.is_empty() && !force {
            return Ok(JavaDeleteOutcome::RequiresConfirmation {
                instances: references,
            });
        }
        let home = self.java_home_of(environment_id)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE managed_java_environments SET status = 'deleted' WHERE id = ?1 AND status <> 'deleted'",
            params![environment_id],
        )?;
        transaction.commit()?;
        if changed == 0 {
            return Err(CoreError::InvalidStoredState(
                "环境不存在或已经被删除".to_owned(),
            ));
        }
        let files_removed = self.remove_managed_java_files(&home)?;
        Ok(JavaDeleteOutcome::Deleted { files_removed })
    }

    /// 重启后收敛中断的删除：重新清理墓碑环境的残留文件。
    pub(crate) fn recover_java_deletions(&self) -> Result<()> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, home_directory FROM managed_java_environments WHERE status = 'deleted'",
        )?;
        let homes = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);
        drop(connection);
        for (_id, home) in homes {
            let _ = self.remove_managed_java_files(&home)?;
        }
        Ok(())
    }

    /// 健康验证：环境目录与 bin/java.exe 存在。
    pub fn verify_java_environment(&self, environment_id: &str) -> Result<bool> {
        let home = self.java_home_of(environment_id)?;
        Ok(Path::new(&home).join("bin/java.exe").is_file())
    }

    /// 为实例指派 Java 环境：主版本必须一致,数据库与磁盘运行时清单同步更新。
    pub fn set_instance_java_environment(
        &self,
        instance_id: &str,
        environment_id: &str,
    ) -> Result<()> {
        let environment = self
            .list_managed_java()?
            .into_iter()
            .find(|environment| environment.id == environment_id)
            .ok_or_else(|| CoreError::InvalidStoredState("Java 环境不存在".to_owned()))?;
        if environment.status != JavaEnvironmentStatus::Ready {
            return Err(CoreError::InvalidStoredState(
                "只能指派已就绪的 Java 环境".to_owned(),
            ));
        }
        let (plan_json, instance_root) = self.instance_plan_and_root(instance_id)?;
        let plan: serde_json::Value = serde_json::from_str(&plan_json)?;
        let required_major = plan
            .pointer("/game/javaMajorVersion")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| CoreError::InvalidStoredState("实例计划缺少 Java 主版本".to_owned()))?;
        let environment_major = java_major_of(&environment.full_version)?;
        if environment_major != required_major {
            return Err(CoreError::InvalidStoredState(format!(
                "主版本不一致：实例需要 Java {required_major}，所选环境为 Java {environment_major}"
            )));
        }
        let runtime_json = self.instance_runtime_json(instance_id)?;
        let mut runtime: serde_json::Value = serde_json::from_str(&runtime_json)?;
        runtime["javaHome"] = serde_json::Value::String(environment.home_directory.clone());
        let runtime_text = serde_json::to_string(&runtime)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "
            UPDATE instance_runtime
            SET java_environment_id = ?2, runtime_json = ?3
            WHERE instance_id = ?1
            ",
            params![instance_id, environment_id, runtime_text],
        )?;
        transaction.commit()?;
        let disk_manifest = Path::new(&instance_root).join(".moyumax/runtime.json");
        write_json_atomically(&disk_manifest, runtime_text.as_bytes())?;
        Ok(())
    }

    /// 解析恢复包：同一发行版与主版本线的最新可用补丁（联网）。
    pub async fn resolve_java_restore_package(
        &self,
        metadata: &MetadataClient,
        environment_id: &str,
    ) -> Result<crate::ResolvedJavaPackage> {
        let major = self
            .list_deleted_java_environments()?
            .into_iter()
            .find(|environment| environment.id == environment_id)
            .map(|environment| java_major_of(&environment.full_version))
            .ok_or_else(|| CoreError::InvalidStoredState("该环境未被删除或不存在".to_owned()))??;
        metadata
            .resolve_zulu_jdk(
                u16::try_from(major)
                    .map_err(|_| CoreError::InvalidStoredState("Java 主版本超出范围".to_owned()))?,
            )
            .await
    }

    /// 一键恢复：下载并解包恢复包（经来源策略校验）,
    /// 恢复后把引用旧环境的实例指到可用环境。
    pub async fn restore_java_environment(
        &self,
        package: &crate::ResolvedJavaPackage,
        downloader: &ArtifactDownloader,
        environment_id: &str,
    ) -> Result<JavaEnvironmentSummary> {
        let deleted = self
            .list_deleted_java_environments()?
            .into_iter()
            .find(|environment| environment.id == environment_id)
            .ok_or_else(|| CoreError::InvalidStoredState("该环境未被删除或不存在".to_owned()))?;
        let data_directory = self.selected_data_directory()?;
        let staging = data_directory
            .join(".staging")
            .join("java-restore")
            .join(environment_id);
        let _ = fs::remove_dir_all(&staging);
        let report = downloader
            .fetch_with_policy(
                &package.artifact,
                &staging,
                &staging,
                &SourcePolicy::default(),
                None,
            )
            .await?;
        let home = data_directory
            .join("store")
            .join("java")
            .join(package.distribution.directory_name())
            .join(&package.full_version)
            .join(package.architecture.database_value());
        if home.exists() {
            fs::remove_dir_all(&home)?;
        }
        let archive = report.result.staged_file;
        let archive_for_worker = archive.clone();
        let home_for_worker = home.clone();
        tokio::task::spawn_blocking(move || {
            crate::extract_zip_safely(
                &archive_for_worker,
                &home_for_worker,
                crate::ArchiveLimits::java_default(),
            )
        })
        .await
        .map_err(|error| CoreError::Archive(format!("Java 解包线程失败：{error}")))??;
        if !home.join("bin/java.exe").is_file() {
            return Err(CoreError::Archive(
                "Azul JDK 解包后缺少 bin/java.exe".to_owned(),
            ));
        }
        let _ = fs::remove_dir_all(&staging);

        // 恢复或新建可用环境;先记录旧环境的引用,再重定向。
        let affected_instances = self.referencing_instances(environment_id)?;
        let restored_id = format!("{}-restored", deleted.id);
        let restored = crate::ManagedJavaEnvironment {
            id: restored_id.clone(),
            distribution: deleted.distribution,
            full_version: package.full_version.clone(),
            architecture: deleted.architecture,
            home_directory: home.to_string_lossy().into_owned(),
            status: JavaEnvironmentStatus::Ready,
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<(String, String)> = transaction
            .query_row(
                "SELECT id, status FROM managed_java_environments WHERE distribution = ?1 AND full_version = ?2 AND architecture = ?3",
                params![
                    deleted.distribution.database_value(),
                    package.full_version,
                    deleted.architecture.database_value()
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        let target_id = match existing {
            Some((existing_id, _)) => {
                transaction.execute(
                    "UPDATE managed_java_environments SET home_directory = ?2, status = 'ready' WHERE id = ?1",
                    params![existing_id, restored.home_directory],
                )?;
                existing_id
            }
            None => {
                transaction.execute(
                    "
                    INSERT INTO managed_java_environments (
                        id, distribution, full_version, architecture, home_directory, status
                    ) VALUES (?1, ?2, ?3, ?4, ?5, 'ready')
                    ",
                    params![
                        restored_id,
                        deleted.distribution.database_value(),
                        package.full_version,
                        deleted.architecture.database_value(),
                        restored.home_directory
                    ],
                )?;
                restored_id
            }
        };
        transaction.execute(
            "UPDATE instance_runtime SET java_environment_id = ?2 WHERE java_environment_id = ?1",
            params![environment_id, target_id],
        )?;
        transaction.commit()?;

        // 同步受影响实例的磁盘运行时清单 javaHome。
        for instance in &affected_instances {
            self.rewrite_instance_java_home(&instance.id, &target_id)?;
        }
        // 移除旧墓碑。
        self.connection()?.execute(
            "DELETE FROM managed_java_environments WHERE id = ?1 AND status = 'deleted'",
            params![environment_id],
        )?;
        self.java_environment_summary(&target_id)
    }

    fn java_home_of(&self, environment_id: &str) -> Result<String> {
        self.connection()?
            .query_row(
                "SELECT home_directory FROM managed_java_environments WHERE id = ?1",
                params![environment_id],
                |row| row.get(0),
            )
            .map_err(|_| CoreError::InvalidStoredState("Java 环境不存在".to_owned()))
    }

    fn referencing_instances(
        &self,
        environment_id: &str,
    ) -> Result<Vec<ReferencingInstanceSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT i.id, i.name
            FROM instances i
            JOIN instance_runtime r ON r.instance_id = i.id
            WHERE r.java_environment_id = ?1
            ORDER BY i.name
            ",
        )?;
        let rows = statement.query_map(params![environment_id], |row| {
            Ok(ReferencingInstanceSummary {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(CoreError::from)
    }

    fn instance_plan_and_root(&self, instance_id: &str) -> Result<(String, String)> {
        self.connection()?
            .query_row(
                "SELECT r.plan_json, i.root_directory FROM instance_runtime r JOIN instances i ON i.id = r.instance_id WHERE r.instance_id = ?1",
                params![instance_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| CoreError::InvalidStoredState("实例不存在".to_owned()))
    }

    fn instance_runtime_json(&self, instance_id: &str) -> Result<String> {
        self.connection()?
            .query_row(
                "SELECT runtime_json FROM instance_runtime WHERE instance_id = ?1",
                params![instance_id],
                |row| row.get(0),
            )
            .map_err(|_| CoreError::InvalidStoredState("实例运行时清单不存在".to_owned()))
    }

    /// 把实例运行时清单的 javaHome 重写为指定环境的位置（数据库与磁盘同步）。
    fn rewrite_instance_java_home(&self, instance_id: &str, environment_id: &str) -> Result<()> {
        let home = self.java_home_of(environment_id)?;
        let (_plan_json, instance_root) = self.instance_plan_and_root(instance_id)?;
        let runtime_json = self.instance_runtime_json(instance_id)?;
        let mut runtime: serde_json::Value = serde_json::from_str(&runtime_json)?;
        runtime["javaHome"] = serde_json::Value::String(home);
        let runtime_text = serde_json::to_string(&runtime)?;
        self.connection()?.execute(
            "UPDATE instance_runtime SET runtime_json = ?2 WHERE instance_id = ?1",
            params![instance_id, runtime_text],
        )?;
        let disk_manifest = Path::new(&instance_root).join(".moyumax/runtime.json");
        write_json_atomically(&disk_manifest, runtime_text.as_bytes())?;
        Ok(())
    }

    /// 只清理受管 Java 根内的环境文件;越界路径拒绝删除。
    fn remove_managed_java_files(&self, home: &str) -> Result<bool> {
        let java_root = self.selected_data_directory()?.join("store").join("java");
        let home_path = Path::new(home);
        if !home_path.starts_with(&java_root) {
            return Ok(false);
        }
        if home_path == java_root {
            return Err(CoreError::InvalidStoredState(
                "拒绝清理整个 Java 根目录".to_owned(),
            ));
        }
        if home_path.exists() {
            fs::remove_dir_all(home_path)?;
        }
        Ok(true)
    }

    fn java_environment_summary(&self, environment_id: &str) -> Result<JavaEnvironmentSummary> {
        self.list_java_environments()?
            .into_iter()
            .find(|environment| environment.id == environment_id)
            .ok_or_else(|| CoreError::InvalidStoredState("Java 环境不存在".to_owned()))
    }
}

fn java_major_of(full_version: &str) -> Result<u64> {
    let major_text = full_version.split(['.', '+', '-']).next().ok_or_else(|| {
        CoreError::InvalidStoredState(format!("Java 版本格式无效:{full_version}"))
    })?;
    major_text
        .parse::<u64>()
        .map_err(|_| CoreError::InvalidStoredState(format!("Java 主版本无法解析:{full_version}")))
}

fn directory_size(path: &Path) -> u64 {
    if !path.is_dir() {
        return 0;
    }
    let mut total = 0_u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(directory) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&directory) {
            for entry in entries.flatten() {
                let metadata = entry.metadata().ok();
                match metadata {
                    Some(meta) if meta.is_dir() => stack.push(entry.path()),
                    Some(meta) => total = total.saturating_add(meta.len()),
                    None => {}
                }
            }
        }
    }
    total
}

fn write_json_atomically(path: &Path, payload: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, payload)?;
    fs::rename(temporary, path)?;
    Ok(())
}
