//! live:逐文件细粒度下载探针(前 12 个文件,全候选链),`--ignored` 手动运行。
#![allow(missing_docs)]

use std::{path::PathBuf, time::Instant};

use moyumax_core::{AppService, MciMirrorClient, SourceAttemptOutcome, parse_modpack_archive};

use tempfile::TempDir;

const PACK_PATH: &str = "D:\\Downloads\\你好,新蒸程！V1.5.9.zip";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live:细粒度探针"]
async fn live_modpack_file_probe() {
    let directory = TempDir::new().unwrap();
    let service = AppService::open(
        &directory.path().join("state.sqlite3"),
        &directory.path().join("data"),
    )
    .unwrap();
    service.skip_onboarding().unwrap();

    let pack = PathBuf::from(PACK_PATH);
    let plan = parse_modpack_archive(&pack).unwrap();
    let mci = MciMirrorClient::new().unwrap();
    let artifacts = service
        .resolve_modpack_artifacts(&plan, &mci, &|_, _, _| {})
        .await
        .unwrap();
    let downloader = moyumax_core::ArtifactDownloader::new(6).unwrap();
    let staging = directory.path().join("staging");
    let shared = directory.path().join("shared");

    for (relative, artifact, candidates) in artifacts.into_iter().take(12) {
        let started = Instant::now();
        let outcome = downloader
            .fetch_with_candidates(&artifact, &staging, &shared, &candidates, None)
            .await;
        let elapsed = started.elapsed().as_secs_f64();
        match outcome {
            Ok(report) => {
                let mib = report.result.bytes as f64 / 1024.0 / 1024.0;
                eprintln!(
                    "OK {relative} {mib:.2} MiB {elapsed:.1}s {:.0} KB/s via {} seg={}",
                    report.result.bytes as f64 / 1024.0 / elapsed,
                    report.final_label,
                    report.segment_count,
                );
                for attempt in &report.attempts {
                    let mark = match &attempt.outcome {
                        SourceAttemptOutcome::Success => "ok".to_owned(),
                        SourceAttemptOutcome::Failed { error } => format!("fail {error}"),
                    };
                    eprintln!("    attempt {} -> {mark}", attempt.url);
                }
            }
            Err(error) => eprintln!("ERR {relative} {elapsed:.1}s: {error}"),
        }
    }
}
