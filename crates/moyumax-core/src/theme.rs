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
const SETTING_UI_THEME_PACK: &str = "ui_theme_pack";
const MAX_BACKGROUND_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_THEME_PACK_BYTES: u64 = 256 * 1024;
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

// ============================================================
// 主题包标准 v2:基础声明(tokens+rules)+ 特殊样式(overrides),
// 跨版本契约与按版本/页面定制,见 docs/theme-standard.md。
// ============================================================

/// 主题包 v2 允许覆盖的视觉令牌(跨版本稳定契约)。
const THEME_V2_TOKENS: &[&str] = &[
    "--bg-0",
    "--bg-1",
    "--bg-2",
    "--bg-grad",
    "--glass",
    "--glass-strong",
    "--glass-border",
    "--glass-highlight",
    "--glass-blur",
    "--text-1",
    "--text-2",
    "--text-3",
    "--accent",
    "--accent-ink",
    "--accent-soft",
    "--ok",
    "--ok-soft",
    "--warn",
    "--warn-soft",
    "--danger",
    "--danger-soft",
    "--info",
    "--info-soft",
    "--r",
    "--shadow-1",
    "--shadow-2",
    "--font",
    "--mono",
];

const THEME_V2_PROPERTIES: &[&str] = &[
    "color",
    "background",
    "background-color",
    "background-image",
    "border",
    "border-color",
    "border-width",
    "border-style",
    "border-radius",
    "outline",
    "outline-color",
    "box-shadow",
    "text-shadow",
    "backdrop-filter",
    "-webkit-backdrop-filter",
    "filter",
    "opacity",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "gap",
    "row-gap",
    "column-gap",
    "width",
    "min-width",
    "max-width",
    "height",
    "min-height",
    "max-height",
    "top",
    "right",
    "bottom",
    "left",
    "inset",
    "font",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "line-height",
    "letter-spacing",
    "text-align",
    "text-decoration",
    "text-transform",
    "white-space",
    "word-break",
    "overflow-wrap",
    "display",
    "flex",
    "flex-direction",
    "flex-wrap",
    "align-items",
    "align-content",
    "justify-content",
    "align-self",
    "place-items",
    "place-content",
    "grid-template-columns",
    "grid-column",
    "grid-row",
    "overflow",
    "overflow-x",
    "overflow-y",
    "transition",
    "transition-property",
    "transition-duration",
    "transition-timing-function",
    "transform",
    "cursor",
    "pointer-events",
    "user-select",
    "visibility",
    "content",
    "object-fit",
    "image-rendering",
    "aspect-ratio",
    "flex-grow",
    "flex-shrink",
    "flex-basis",
    "order",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeVersionRange {
    pub min: Option<String>,
    pub max: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeRule {
    pub selector: String,
    pub declarations: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeOverride {
    pub name: String,
    pub pages: Option<Vec<String>>,
    pub app_version: Option<ThemeVersionRange>,
    pub rules: Vec<ThemeRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeBase {
    pub tokens: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub rules: Vec<ThemeRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePackV2 {
    pub format_version: u16,
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: Option<String>,
    pub app_version: Option<ThemeVersionRange>,
    pub base: ThemeBase,
    #[serde(default)]
    pub overrides: Vec<ThemeOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePackMeta {
    pub id: String,
    pub name: String,
    pub author: String,
    pub builtin: bool,
}

fn validate_theme_pack_v2(pack: &ThemePackV2) -> Result<()> {
    if pack.format_version != 2 {
        return Err(CoreError::Content(format!(
            "不支持的主题包格式版本：{}",
            pack.format_version
        )));
    }
    if pack.id.is_empty()
        || pack.id.len() > 32
        || !pack.id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(CoreError::Content(
            "主题包 id 必须是 1-32 位小写字母/数字/连字符".to_owned(),
        ));
    }
    validate_plain_text(&pack.name, "主题包名称")?;
    validate_plain_text(&pack.author, "主题包作者")?;
    if let Some(description) = &pack.description {
        validate_plain_text(description, "主题包描述")?;
    }
    if pack.base.tokens.is_empty() && pack.base.rules.is_empty() && pack.overrides.is_empty() {
        return Err(CoreError::Content("主题包没有任何样式声明".to_owned()));
    }
    for token in pack.base.tokens.keys() {
        if !THEME_V2_TOKENS.contains(&token.as_str()) {
            return Err(CoreError::Content(format!("主题包含非契约令牌：{token}")));
        }
        validate_declaration_value(&pack.base.tokens[token])?;
    }
    for rule in &pack.base.rules {
        validate_theme_rule(rule)?;
    }
    for (index, override_item) in pack.overrides.iter().enumerate() {
        validate_plain_text(&override_item.name, "特殊样式名称")?;
        if let Some(pages) = &override_item.pages {
            for page in pages {
                if !THEME_V2_PAGES.contains(&page.as_str()) {
                    return Err(CoreError::Content(format!(
                        "特殊样式 #{} 包含未知页面键：{page}",
                        index + 1
                    )));
                }
            }
        }
        for rule in &override_item.rules {
            validate_theme_rule(rule)?;
        }
    }
    Ok(())
}

const THEME_V2_PAGES: &[&str] = &[
    "home",
    "instances",
    "instanceDetail",
    "resources",
    "tasks",
    "data",
    "accounts",
    "settings",
    "onboarding",
    "netplay",
    "backups",
    "crash",
];

fn validate_theme_rule(rule: &ThemeRule) -> Result<()> {
    if rule.selector.len() > 120 || rule.selector.trim().is_empty() {
        return Err(CoreError::Content("选择器为空或超过 120 字符".to_owned()));
    }
    for token in rule.selector.split_whitespace() {
        validate_selector_token(token)?;
    }
    if rule.declarations.len() > 32 {
        return Err(CoreError::Content("单条规则声明过多(>32)".to_owned()));
    }
    for (property, value) in &rule.declarations {
        if !THEME_V2_PROPERTIES.contains(&property.as_str()) {
            return Err(CoreError::Content(format!("不允许的样式属性：{property}")));
        }
        validate_declaration_value(value)?;
    }
    Ok(())
}

fn validate_selector_token(token: &str) -> Result<()> {
    let reject =
        |reason: &str| CoreError::Content(format!("选择器包含不允许的片段 {token}：{reason}"));
    if token == ">" || token == "+" || token == "~" {
        return Ok(());
    }
    if token.contains(['#', '*', '[', ']'])
        || token.contains("url(")
        || token.contains("javascript:")
    {
        return Err(reject("仅允许类选择器与文本"));
    }
    let body = token
        .split(':')
        .next()
        .unwrap_or(token)
        .replace(".window", "");
    if !body.is_empty()
        && !body.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '(' | ')')
        })
    {
        return Err(reject("类名含非法字符"));
    }
    if !token.contains('.') {
        return Err(reject("缺少类选择器"));
    }
    Ok(())
}

fn validate_declaration_value(value: &str) -> Result<()> {
    if value.len() > 240 {
        return Err(CoreError::Content("样式值超过 240 字符".to_owned()));
    }
    let lowered = value.to_lowercase();
    for banned in ["url(", "@import", "expression", "javascript:", "behavior"] {
        if lowered.contains(banned) {
            return Err(CoreError::Content(format!("样式值包含禁止内容：{banned}")));
        }
    }
    Ok(())
}

/// 解析并校验主题包 v2 JSON。
pub fn parse_theme_pack_v2(source: &str) -> Result<ThemePackV2> {
    let pack: ThemePackV2 = serde_json::from_str(source)
        .map_err(|error| CoreError::Content(format!("主题包不是有效的 JSON：{error}")))?;
    validate_theme_pack_v2(&pack)?;
    Ok(pack)
}

/// v1 纯配色包 → v2 base.tokens(附录 A 映射,不支持的键忽略)。
pub fn upgrade_theme_pack_v1(pack: &ThemePack) -> ThemePackV2 {
    const MAPPING: &[(&str, &str)] = &[
        ("accent", "--accent"),
        ("text", "--text-1"),
        ("text-2", "--text-2"),
        ("text-3", "--text-3"),
        ("bg-window", "--bg-0"),
        ("bg-app", "--bg-1"),
        ("bg-nav", "--bg-1"),
        ("surface", "--glass-strong"),
        ("surface-2", "--glass-strong"),
        ("border", "--glass-border"),
        ("border-strong", "--glass-border"),
    ];
    let mut tokens = std::collections::BTreeMap::new();
    for (legacy, token) in MAPPING {
        if let Some(value) = pack.colors.get(*legacy) {
            tokens.insert((*token).to_owned(), value.clone());
        }
    }
    ThemePackV2 {
        format_version: 2,
        id: "imported-v1".to_owned(),
        name: pack.name.clone(),
        author: pack.author.clone(),
        description: None,
        app_version: None,
        base: ThemeBase {
            tokens,
            rules: Vec::new(),
        },
        overrides: Vec::new(),
    }
}

impl AppService {
    /// 导入主题包 JSON(v2,或 v1 自动升级):校验后复制到受管 themes/ 目录。
    pub fn import_theme_pack(&self, source_path: &std::path::Path) -> Result<ThemePackMeta> {
        let metadata = fs::metadata(source_path).map_err(|_| {
            CoreError::Content(format!("找不到主题包文件：{}", source_path.display()))
        })?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_THEME_PACK_BYTES {
            return Err(CoreError::Content(
                "主题包必须是 1 字节到 256 KiB 的 JSON 文件".to_owned(),
            ));
        }
        let source = fs::read_to_string(source_path)
            .map_err(|error| CoreError::Content(format!("主题包读取失败：{error}")))?;
        let pack = match parse_theme_pack_v2(&source) {
            Ok(pack) => pack,
            Err(v2_error) => {
                let v1 = parse_theme_pack(&source).map_err(|_| v2_error)?;
                upgrade_theme_pack_v1(&v1)
            }
        };
        let directory = self.selected_data_directory()?.join("themes");
        fs::create_dir_all(&directory)?;
        fs::write(directory.join(format!("{}.json", pack.id)), &source)?;
        Ok(ThemePackMeta {
            id: pack.id,
            name: pack.name,
            author: pack.author,
            builtin: false,
        })
    }

    /// 已导入主题包元数据清单(内置包由前端注册,不在此列出)。
    pub fn list_imported_theme_packs(&self) -> Result<Vec<ThemePackMeta>> {
        let directory = self.selected_data_directory()?.join("themes");
        let mut metas = Vec::new();
        if directory.is_dir() {
            for entry in fs::read_dir(&directory)? {
                let path = entry?.path();
                if path.extension().and_then(|value| value.to_str()) != Some("json") {
                    continue;
                }
                let source = fs::read_to_string(&path)?;
                if let Ok(pack) = parse_theme_pack_v2(&source) {
                    metas.push(ThemePackMeta {
                        id: pack.id,
                        name: pack.name,
                        author: pack.author,
                        builtin: false,
                    });
                }
            }
        }
        metas.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(metas)
    }

    /// 读取已导入主题包的 JSON 源文本(前端引擎编译用)。
    pub fn read_theme_pack(&self, pack_id: &str) -> Result<String> {
        if pack_id.is_empty()
            || !pack_id.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
        {
            return Err(CoreError::Content("主题包 id 无效".to_owned()));
        }
        let path = self
            .selected_data_directory()?
            .join("themes")
            .join(format!("{pack_id}.json"));
        if !path.is_file() {
            return Err(CoreError::Content("主题包不存在或已被删除".to_owned()));
        }
        Ok(fs::read_to_string(path)?)
    }

    pub fn remove_theme_pack(&self, pack_id: &str) -> Result<()> {
        if pack_id.is_empty()
            || !pack_id.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
        {
            return Err(CoreError::Content("主题包 id 无效".to_owned()));
        }
        let path = self
            .selected_data_directory()?
            .join("themes")
            .join(format!("{pack_id}.json"));
        if path.is_file() {
            fs::remove_file(path)?;
        }
        if self.ui_theme_pack()? == pack_id {
            self.set_ui_theme_pack("default")?;
        }
        Ok(())
    }

    /// 当前启用的主题包 id("default" 为内置默认;其余为内置特色包或已导入包)。
    pub fn ui_theme_pack(&self) -> Result<String> {
        let connection = self.connection()?;
        Ok(read_setting(&connection, SETTING_UI_THEME_PACK)?
            .unwrap_or_else(|| "default".to_owned()))
    }

    pub fn set_ui_theme_pack(&self, pack_id: &str) -> Result<()> {
        if pack_id.is_empty()
            || pack_id.len() > 32
            || !pack_id.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
        {
            return Err(CoreError::Content("主题包 id 无效".to_owned()));
        }
        let connection = self.connection()?;
        write_setting(&connection, SETTING_UI_THEME_PACK, pack_id)?;
        Ok(())
    }
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
