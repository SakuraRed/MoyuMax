//! 自定义背景与纯数据主题包。
//!
//! 主题包是只含配色的 JSON（formatVersion=1），不得包含 CSS、脚本、布局
//! 或任何 URL 形式字符串；颜色键必须在允许清单内且为 #rrggbb。背景设置
//! 持久化在 app_settings，图片复制到受管 backgrounds/ 目录。

use std::{fs, path::PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppService, CoreError, Result, read_setting, write_setting};

const SETTING_UI_BACKGROUND: &str = "ui_background";
const MAX_BACKGROUND_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const THEME_COLOR_TOKENS: &[&str] = &[
    "bg-window",
    "bg-app",
    "bg-nav",
    "bg-statusbar",
    "surface",
    "surface-2",
    "surface-3",
    "border",
    "border-strong",
    "text",
    "text-2",
    "text-3",
    "accent",
    "accent-strong",
    "on-accent",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePack {
    pub format_version: u16,
    pub name: String,
    pub author: String,
    pub colors: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum UiBackground {
    #[serde(rename = "default")]
    #[default]
    Default,
    #[serde(rename = "color")]
    Color { color: String },
    #[serde(rename = "image")]
    Image { file: String },
    #[serde(rename = "themePack")]
    ThemePack { pack: ThemePack },
}

/// 解析并校验主题包 JSON；任何越界内容都被拒绝。
pub fn parse_theme_pack(source: &str) -> Result<ThemePack> {
    let pack: ThemePack = serde_json::from_str(source)
        .map_err(|error| CoreError::Content(format!("主题包不是有效的 JSON：{error}")))?;
    validate_theme_pack(&pack)?;
    Ok(pack)
}

fn validate_theme_pack(pack: &ThemePack) -> Result<()> {
    if pack.format_version != 1 {
        return Err(CoreError::Content(format!(
            "不支持的主题包格式版本：{}",
            pack.format_version
        )));
    }
    validate_plain_text(&pack.name, "主题包名称")?;
    validate_plain_text(&pack.author, "主题包作者")?;
    if pack.colors.is_empty() {
        return Err(CoreError::Content("主题包没有颜色定义".to_owned()));
    }
    if pack.colors.len() > THEME_COLOR_TOKENS.len() {
        return Err(CoreError::Content("主题包颜色条目过多".to_owned()));
    }
    for (token, value) in &pack.colors {
        if !THEME_COLOR_TOKENS.contains(&token.as_str()) {
            return Err(CoreError::Content(format!(
                "主题包包含不允许的配色键：{token}"
            )));
        }
        validate_color_value(value)?;
    }
    Ok(())
}

fn validate_plain_text(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty()
        || value.chars().count() > 64
        || value.chars().any(char::is_control)
        || value.contains("://")
    {
        return Err(CoreError::Content(format!(
            "{label}不安全：不能为空、超过 64 字符或包含 URL"
        )));
    }
    Ok(())
}

fn validate_color_value(value: &str) -> Result<()> {
    let valid = value.len() == 7
        && value.starts_with('#')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit());
    if !valid {
        return Err(CoreError::Content(format!(
            "颜色必须是 #rrggbb 形式：{value}"
        )));
    }
    Ok(())
}

fn validate_hex_color(value: &str, label: &str) -> Result<()> {
    validate_color_value(value)
        .map_err(|_| CoreError::Content(format!("{label}必须是 #rrggbb 形式")))
}

impl AppService {
    /// 当前背景设置（默认 default；非法存储值按默认处理）。
    pub fn ui_background(&self) -> Result<UiBackground> {
        let connection = self.connection()?;
        let stored = read_setting(&connection, SETTING_UI_BACKGROUND)?;
        let Some(stored) = stored else {
            return Ok(UiBackground::Default);
        };
        let Ok(background) = serde_json::from_str::<UiBackground>(&stored) else {
            return Ok(UiBackground::Default);
        };
        Ok(background)
    }

    pub fn set_ui_background(&self, background: &UiBackground) -> Result<()> {
        match background {
            UiBackground::Default => {}
            UiBackground::Color { color } => validate_hex_color(color, "背景颜色")?,
            UiBackground::Image { file } => {
                let path = self.background_image_path(file)?;
                if !path.is_file() {
                    return Err(CoreError::Content("背景图片不存在，请重新选择".to_owned()));
                }
            }
            UiBackground::ThemePack { pack } => validate_theme_pack(pack)?,
        }
        let connection: Connection = self.connection()?;
        write_setting(
            &connection,
            SETTING_UI_BACKGROUND,
            &serde_json::to_string(background)?,
        )?;
        Ok(())
    }

    /// 导入本地图片为背景：校验类型与大小后复制到受管目录。
    pub fn import_background_image(&self, source_path: &std::path::Path) -> Result<UiBackground> {
        let metadata = fs::metadata(source_path).map_err(|_| {
            CoreError::Content(format!("找不到背景图片：{}", source_path.display()))
        })?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_BACKGROUND_IMAGE_BYTES
        {
            return Err(CoreError::Content(
                "背景图片必须是 1 字节到 8 MiB 的文件".to_owned(),
            ));
        }
        let extension = source_path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .filter(|value| matches!(value.as_str(), "png" | "jpg" | "jpeg" | "webp"))
            .ok_or_else(|| CoreError::Content("背景图片必须是 PNG、JPG 或 WebP".to_owned()))?;
        let file = format!("background-{}.{}", Uuid::new_v4().simple(), extension);
        let target = self.background_image_path(&file)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source_path, &target)?;
        let background = UiBackground::Image { file };
        self.set_ui_background(&background)?;
        Ok(background)
    }

    /// 读取背景图片字节（用于界面渲染）。
    pub fn read_background_image(&self) -> Result<Option<(String, Vec<u8>)>> {
        let UiBackground::Image { file } = self.ui_background()? else {
            return Ok(None);
        };
        let path = self.background_image_path(&file)?;
        if !path.is_file() {
            return Ok(None);
        }
        let mime = match path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            _ => "image/jpeg",
        };
        Ok(Some((mime.to_owned(), fs::read(&path)?)))
    }

    fn background_image_path(&self, file: &str) -> Result<PathBuf> {
        if file.is_empty()
            || file.chars().any(|character| {
                matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                ) || character.is_control()
            })
        {
            return Err(CoreError::Content("背景图片文件名不安全".to_owned()));
        }
        Ok(self
            .selected_data_directory()?
            .join("backgrounds")
            .join(file))
    }
}
