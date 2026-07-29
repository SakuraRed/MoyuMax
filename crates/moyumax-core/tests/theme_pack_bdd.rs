use moyumax_core::{AppService, ThemePack, parse_theme_pack_v2, upgrade_theme_pack_v1};
use tempfile::TempDir;

fn valid_pack_source() -> String {
    serde_json::json!({
        "formatVersion": 2,
        "id": "animal-island",
        "name": "动物森友会",
        "author": "MoyuMax",
        "description": "奶油纸面与自然绿",
        "appVersion": { "min": "0.1.0" },
        "base": {
            "tokens": { "--bg-0": "#fdf8ee", "--accent": "#7fb069", "--r": "16px" },
            "rules": [
                { "selector": ".panel", "declarations": { "background": "rgba(255,252,245,0.92)", "border-radius": "18px" } },
                { "selector": ".btn.primary:hover", "declarations": { "filter": "brightness(1.06)" } }
            ]
        },
        "overrides": [
            {
                "name": "home-hero",
                "pages": ["home"],
                "appVersion": { "min": "0.1.0" },
                "rules": [ { "selector": ".hero-card", "declarations": { "border-radius": "20px" } } ]
            }
        ]
    })
    .to_string()
}

#[test]
fn v2_valid_pack_parses_with_base_and_overrides() {
    let pack = parse_theme_pack_v2(&valid_pack_source()).expect("合法主题包必须通过");
    assert_eq!(pack.id, "animal-island");
    assert_eq!(pack.base.tokens.len(), 3);
    assert_eq!(pack.base.rules.len(), 2);
    assert_eq!(pack.overrides.len(), 1);
    assert_eq!(pack.overrides[0].pages, Some(vec!["home".to_owned()]));
}

#[test]
fn v2_rejects_unknown_token_property_and_selector() {
    let bad_token = valid_pack_source().replace("--bg-0", "--evil");
    assert!(parse_theme_pack_v2(&bad_token).is_err());

    let bad_property = valid_pack_source().replace("border-radius", "position");
    assert!(parse_theme_pack_v2(&bad_property).is_err());

    let bad_selector = valid_pack_source().replace(".btn.primary:hover", "#hero");
    assert!(parse_theme_pack_v2(&bad_selector).is_err());

    let bad_value =
        valid_pack_source().replace("rgba(255,252,245,0.92)", "url(https://evil.example/x.css)");
    assert!(parse_theme_pack_v2(&bad_value).is_err());

    let bad_page = valid_pack_source().replace("\"home\"", "\"moon\"");
    assert!(parse_theme_pack_v2(&bad_page).is_err());
}

#[test]
fn v1_pack_upgrades_to_v2_tokens() {
    let mut colors = std::collections::BTreeMap::new();
    colors.insert("accent".to_owned(), "#7cc46c".to_owned());
    colors.insert("text".to_owned(), "#f2f2ef".to_owned());
    colors.insert("surface".to_owned(), "#25252a".to_owned());
    let v1 = ThemePack {
        format_version: 1,
        name: "旧绿".to_owned(),
        author: "tester".to_owned(),
        colors,
    };
    let v2 = upgrade_theme_pack_v1(&v1);
    assert_eq!(v2.format_version, 2);
    assert_eq!(v2.base.tokens.get("--accent"), Some(&"#7cc46c".to_owned()));
    assert_eq!(v2.base.tokens.get("--text-1"), Some(&"#f2f2ef".to_owned()));
    assert_eq!(
        v2.base.tokens.get("--glass-strong"),
        Some(&"#25252a".to_owned())
    );
    assert!(v2.base.rules.is_empty() && v2.overrides.is_empty());
}

#[test]
fn docs_template_pack_is_valid() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/examples/theme-template.json");
    let source = std::fs::read_to_string(&path).expect("主题模板文件必须存在");
    let pack = parse_theme_pack_v2(&source).expect("docs 主题模板必须通过 v2 校验");
    assert_eq!(pack.id, "my-theme");
    assert!(!pack.base.tokens.is_empty());
    assert!(!pack.overrides.is_empty());
}

#[test]
fn import_list_read_remove_roundtrip() {
    let directory = TempDir::new().unwrap();
    let service = AppService::open(
        &directory.path().join("state.sqlite3"),
        &directory.path().join("data"),
    )
    .unwrap();
    service.skip_onboarding().unwrap();
    let source_path = directory.path().join("pack.json");
    std::fs::write(&source_path, valid_pack_source()).unwrap();

    let meta = service
        .import_theme_pack(&source_path)
        .expect("导入必须成功");
    assert_eq!(meta.id, "animal-island");

    let listed = service.list_imported_theme_packs().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "动物森友会");

    let source = service.read_theme_pack("animal-island").unwrap();
    assert!(source.contains("animal-island"));

    service.set_ui_theme_pack("animal-island").unwrap();
    assert_eq!(service.ui_theme_pack().unwrap(), "animal-island");

    service.remove_theme_pack("animal-island").unwrap();
    assert!(service.list_imported_theme_packs().unwrap().is_empty());
    assert_eq!(
        service.ui_theme_pack().unwrap(),
        "default",
        "删除启用包后回落默认"
    );
}
