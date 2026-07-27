//! live:system-proxy 与 no_proxy 对照,`--ignored` 手动运行。
#![allow(missing_docs)]

const URL: &str = "https://mod.mcimirror.top/data/OypNE65K/versions/N9ffXNy0/tacz-neoforge-1.21.1-1.1.8-hotfix-r1.jar";

#[tokio::test]
#[ignore = "live:代理对照探针"]
async fn live_proxy_behavior_probe() {
    // 1) 生产同款:system-proxy 默认(reqwest 读注册表/环境)。
    let system_client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(8))
        .user_agent("MoyuMax/probe")
        .build()
        .unwrap();
    match system_client.get(URL).send().await {
        Ok(response) => eprintln!("system-proxy: {}", response.status()),
        Err(error) => eprintln!("system-proxy ERR: {error}"),
    }
    // 2) 明确 no_proxy。
    let direct_client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(8))
        .user_agent("MoyuMax/probe")
        .no_proxy()
        .build()
        .unwrap();
    match direct_client.get(URL).send().await {
        Ok(response) => eprintln!("no_proxy: {}", response.status()),
        Err(error) => eprintln!("no_proxy ERR: {error}"),
    }
}
