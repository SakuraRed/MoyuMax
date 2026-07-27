//! live:用生产同款 reqwest 配置诊断版本元数据链路,`--ignored` 手动运行。
#![allow(missing_docs)]

use moyumax_core::{MetadataClient, SourcePolicy};

fn print_error_chain(label: &str, error: &reqwest::Error) {
    eprintln!("--- {label} ---");
    eprintln!("top: {error}");
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        eprintln!("caused by: {cause}");
        source = std::error::Error::source(cause);
    }
}

#[tokio::test]
#[ignore = "live:诊断 BMCLAPI 与官方元数据链路"]
async fn live_probe_version_metadata_chain() {
    let url = "https://bmclapi2.bangbang93.com/v1/packages/67e466e82c012158c8cda81df39aa40a7ade7276/1.21.1.json";
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("MoyuMax/probe")
        .no_proxy()
        .build()
        .unwrap();
    match client.get(url).send().await {
        Ok(response) => {
            eprintln!("status: {}", response.status());
            eprintln!("final url: {}", response.url());
            let bytes = response.bytes().await;
            eprintln!("body: {:?}", bytes.map(|b| b.len()));
        }
        Err(error) => print_error_chain("bmclapi no_proxy", &error),
    }

    // 直连偏好 + 镜像优先策略的完整生产路径。
    moyumax_core::set_active_proxy_preference(moyumax_core::ProxyPreference::Direct);
    let metadata = MetadataClient::new()
        .unwrap()
        .with_source_policy(SourcePolicy::MirrorFirst);
    match metadata.fetch_version_catalog().await {
        Ok(catalog) => eprintln!("catalog ok: {} versions", catalog.versions.len()),
        Err(error) => eprintln!("catalog err: {error}"),
    }
}
