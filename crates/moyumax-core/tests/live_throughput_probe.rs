//! live:reqwest(rustls) 与 curl 的单连接吞吐对比,`--ignored` 手动运行。
#![allow(missing_docs)]

use std::time::Instant;

const TARGETS: &[&str] = &[
    "https://cdn.modrinth.com/data/OypNE65K/versions/N9ffXNy0/tacz-neoforge-1.21.1-1.1.8-hotfix-r1.jar",
    "https://mod.mcimirror.top/data/OypNE65K/versions/N9ffXNy0/tacz-neoforge-1.21.1-1.1.8-hotfix-r1.jar",
    "https://mediafilez.forgecdn.net/files/8167/430/tacz-neoforge-1.21.1-1.1.8-hotfix-r1.jar",
];

#[tokio::test]
#[ignore = "live:吞吐探针"]
async fn live_throughput_probe() {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(120))
        .user_agent("MoyuMax/probe")
        .no_proxy()
        .build()
        .unwrap();
    for url in TARGETS {
        let started = Instant::now();
        match client.get(*url).send().await {
            Ok(response) => {
                let mut stream = response.bytes_stream();
                let mut total = 0_u64;
                use futures_util::StreamExt;
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(bytes) => total += bytes.len() as u64,
                        Err(error) => {
                            eprintln!("{url}\n  stream error after {total} bytes: {error}");
                            break;
                        }
                    }
                }
                let elapsed = started.elapsed().as_secs_f64();
                eprintln!(
                    "{url}\n  {} bytes in {elapsed:.1}s = {:.0} KB/s",
                    total,
                    total as f64 / 1024.0 / elapsed
                );
            }
            Err(error) => eprintln!("{url}\n  request error: {error}"),
        }
    }
}
