use std::fs;

use moyumax_core::{AppService, UiBackground, parse_theme_pack};
use tempfile::TempDir;

fn valid_pack_json() -> String {
    serde_json::json!({
        "formatVersion": 1,
        "name": "苔原",
        "author": "Moyu",
        "colors": {
            "bg-app": "#101410",
            "accent": "#7cc46c",
            "text": "#f2f2ef"
        }
    })
    .to_string()
}

#[test]
fn m28_pack_001_valid_theme_pack_parses() {
    let pack = parse_theme_pack(&valid_pack_json()).unwrap();
    assert_eq!(pack.name, "苔原");
    assert_eq!(pack.colors.len(), 3);
    assert_eq!(pack.colors["bg-app"], "#101410");
}

#[test]
fn m28_pack_002_malformed_packs_are_rejected() {
    for (source, reason) in [
        ("not json", "不是有效的 JSON"),
        (
            r##"{"formatVersion":2,"name":"a","author":"b","colors":{"text":"#ffffff"}}"##,
            "格式版本",
        ),
        (
            r##"{"formatVersion":1,"name":"a","author":"b","colors":{"evil-token":"#ffffff"}}"##,
            "不允许的配色键",
        ),
        (
            r##"{"formatVersion":1,"name":"a","author":"b","colors":{"text":"red"}}"##,
            "#rrggbb",
        ),
        (
            r##"{"formatVersion":1,"name":"https://evil.example/x","author":"b","colors":{"text":"#ffffff"}}"##,
            "URL",
        ),
        (
            r##"{"formatVersion":1,"name":"a","author":"b","colors":{}}"##,
            "没有颜色定义",
        ),
    ] {
        let error = parse_theme_pack(source).expect_err(&format!("应拒绝：{reason}"));
        assert!(
            error.to_string().contains(reason),
            "「{reason}」的错误信息不符：{error}"
        );
    }
}

#[test]
fn m28_bg_001_default_and_color_background_persist() {
    let fixture = ThemeFixture::new();
    assert_eq!(
        fixture.service.ui_background().unwrap(),
        UiBackground::Default
    );

    let background = UiBackground::Color {
        color: "#203040".to_owned(),
    };
    fixture.service.set_ui_background(&background).unwrap();
    let reopened = AppService::open(&fixture.database_path, &fixture.data_directory).unwrap();
    assert_eq!(reopened.ui_background().unwrap(), background);

    assert!(
        fixture
            .service
            .set_ui_background(&UiBackground::Color {
                color: "blue".to_owned(),
            })
            .is_err(),
        "非 #rrggbb 颜色必须被拒绝"
    );
}

#[test]
fn m28_bg_002_image_import_validates_type_and_size() {
    let fixture = ThemeFixture::new();
    let source = fixture.directory.path().join("wall.png");
    fs::write(&source, b"\x89PNG-fake-image").unwrap();

    let background = fixture.service.import_background_image(&source).unwrap();
    let UiBackground::Image { file } = &background else {
        panic!("应为图片背景");
    };
    let (mime, bytes) = fixture
        .service
        .read_background_image()
        .unwrap()
        .expect("应能读回图片");
    assert_eq!(mime, "image/png");
    assert_eq!(bytes, b"\x89PNG-fake-image");
    assert_eq!(fixture.service.ui_background().unwrap(), background);
    assert!(file.starts_with("background-"));

    let bad = fixture.directory.path().join("wall.gif");
    fs::write(&bad, b"GIF89a").unwrap();
    assert!(fixture.service.import_background_image(&bad).is_err());
    assert!(
        fixture
            .service
            .import_background_image(std::path::Path::new("D:\\missing\\wall.png"))
            .is_err()
    );
}

#[test]
fn m28_bg_003_missing_image_background_is_rejected() {
    let fixture = ThemeFixture::new();
    let background = UiBackground::Image {
        file: "background-deadbeef.png".to_owned(),
    };
    assert!(fixture.service.set_ui_background(&background).is_err());
}

struct ThemeFixture {
    directory: TempDir,
    database_path: std::path::PathBuf,
    data_directory: std::path::PathBuf,
    service: AppService,
}

impl ThemeFixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("state.sqlite3");
        let data_directory = directory.path().join("data");
        let service = AppService::open(&database_path, &data_directory).unwrap();
        service.skip_onboarding().unwrap();
        Self {
            directory,
            database_path,
            data_directory,
            service,
        }
    }
}
