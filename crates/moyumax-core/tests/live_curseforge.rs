//! CurseForge 官方 API live 测试：只在设置 MOYUMAX_CF_API_KEY 时运行，
//! 未设置时直接跳过。Key 只从环境变量读取，不写入任何文件。

use moyumax_core::{
    CURSEFORGE_CLASS_MOD, CurseForgeClient, CurseforgeSearchQuery, CurseforgeSortField,
    CurseforgeSortOrder,
};

fn live_client() -> Option<CurseForgeClient> {
    let key = std::env::var("MOYUMAX_CF_API_KEY")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    match key {
        Some(key) => Some(CurseForgeClient::new(Some(key)).expect("CurseForge 客户端构造失败")),
        None => {
            eprintln!("MOYUMAX_CF_API_KEY 未设置，跳过 CurseForge live 测试");
            None
        }
    }
}

#[tokio::test]
#[ignore = "联网且需要 MOYUMAX_CF_API_KEY：search sodium 必须有结果"]
async fn cf_live_search_sodium_returns_hits() {
    let Some(client) = live_client() else { return };
    let page = client
        .search(&CurseforgeSearchQuery {
            query: "sodium".to_owned(),
            class_id: CURSEFORGE_CLASS_MOD,
            game_version: None,
            category_id: None,
            mod_loader: None,
            sort_field: CurseforgeSortField::Popularity,
            sort_order: CurseforgeSortOrder::Desc,
            index: 0,
            page_size: 10,
        })
        .await
        .expect("CurseForge 搜索失败");
    assert!(!page.hits.is_empty(), "sodium 搜索必须有结果");
    assert!(
        page.hits
            .iter()
            .all(|hit| !hit.project_id.is_empty() && !hit.slug.is_empty()),
        "搜索结果必须完整归一化"
    );
}

#[tokio::test]
#[ignore = "联网且需要 MOYUMAX_CF_API_KEY：项目文件可解析且 download-url 可取"]
async fn cf_live_files_and_download_url_resolve() {
    let Some(client) = live_client() else { return };
    let page = client
        .search(&CurseforgeSearchQuery {
            query: "sodium".to_owned(),
            class_id: CURSEFORGE_CLASS_MOD,
            game_version: None,
            category_id: None,
            mod_loader: None,
            sort_field: CurseforgeSortField::Popularity,
            sort_order: CurseforgeSortOrder::Desc,
            index: 0,
            page_size: 10,
        })
        .await
        .expect("CurseForge 搜索失败");
    let project = page
        .hits
        .iter()
        .find(|hit| hit.slug == "sodium")
        .or(page.hits.first())
        .expect("sodium 搜索必须有结果");

    let summary = client
        .project_summary(&project.project_id)
        .await
        .expect("项目详情失败");
    assert_eq!(summary.project_id, project.project_id);

    let files = client
        .project_files(&project.project_id, None, None)
        .await
        .expect("项目文件列表失败");
    assert!(!files.is_empty(), "项目必须有文件");
    assert!(
        files
            .iter()
            .all(|file| !file.id.is_empty() && !file.file_name.is_empty() && file.size > 0),
        "文件必须完整归一化"
    );

    let url = client
        .file_download_url(&project.project_id, &files[0].id)
        .await
        .expect("download-url 必须可取（含 edge 兜底）");
    assert!(url.starts_with("https://"), "下载地址必须是 https：{url}");
}
