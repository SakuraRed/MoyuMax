//! 实例截图浏览：清单、读取与位置解析。
//!
//! 截图只读存放在实例 `.minecraft/screenshots/` 下；删除走回收站统一事务
//! （见 `recycle.rs`），复制到剪贴板由桌面层读取字节后完成。

use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};

use crate::{AppService, CoreError, Result};

const MAX_SCREENSHOT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceScreenshot {
    pub file_name: String,
    pub size_bytes: u64,
    pub taken_at_unix_seconds: i64,
}

impl AppService {
    /// 按文件名倒序列出实例截图（Minecraft 截图文件名内嵌时间戳）。
    pub fn list_instance_screenshots(&self, instance_id: &str) -> Result<Vec<InstanceScreenshot>> {
        let instance = self.ready_instance(instance_id)?;
        let directory = Path::new(&instance.root_directory)
            .join(".minecraft")
            .join("screenshots");
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let metadata = fs::symlink_metadata(&directory)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(CoreError::Content("实例截图目录不安全".to_owned()));
        }
        let mut screenshots = Vec::new();
        for child in fs::read_dir(&directory)? {
            let child = child?;
            let metadata = fs::symlink_metadata(child.path())?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                continue;
            }
            let name = child
                .file_name()
                .into_string()
                .map_err(|_| CoreError::Content("截图文件名包含无效字符".to_owned()))?;
            if !name.to_ascii_lowercase().ends_with(".png") {
                continue;
            }
            let taken = metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .and_then(|duration| i64::try_from(duration.as_secs()).ok())
                .unwrap_or_default();
            screenshots.push(InstanceScreenshot {
                file_name: name,
                size_bytes: metadata.len(),
                taken_at_unix_seconds: taken,
            });
        }
        screenshots.sort_by(|left, right| right.file_name.cmp(&left.file_name));
        Ok(screenshots)
    }

    /// 读取截图字节供复制到剪贴板（限制 64 MiB）。
    pub fn read_instance_screenshot(&self, instance_id: &str, file_name: &str) -> Result<Vec<u8>> {
        let path = self.instance_screenshot_path(instance_id, file_name)?;
        let metadata = fs::metadata(&path)?;
        if metadata.len() > MAX_SCREENSHOT_BYTES {
            return Err(CoreError::Content("截图超过 64 MiB，无法复制".to_owned()));
        }
        Ok(fs::read(&path)?)
    }

    /// 解析截图的真实路径（供系统文件管理器定位）。
    pub fn instance_screenshot_path(&self, instance_id: &str, file_name: &str) -> Result<PathBuf> {
        let instance = self.ready_instance(instance_id)?;
        validate_screenshot_name(file_name)?;
        let path = Path::new(&instance.root_directory)
            .join(".minecraft")
            .join("screenshots")
            .join(file_name);
        if !path.is_file() {
            return Err(CoreError::Content(format!("截图 {file_name} 不存在")));
        }
        Ok(path)
    }
}

fn validate_screenshot_name(file_name: &str) -> Result<()> {
    let path = Path::new(file_name);
    let valid = !file_name.is_empty()
        && file_name.len() <= 240
        && !file_name.chars().any(char::is_control)
        && path.components().count() == 1
        && matches!(path.components().next(), Some(Component::Normal(_)))
        && file_name.to_ascii_lowercase().ends_with(".png");
    if !valid {
        return Err(CoreError::Content(format!(
            "截图文件名不安全或不是 PNG：{file_name}"
        )));
    }
    Ok(())
}
