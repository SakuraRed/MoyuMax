//! live:真实整合包下载基准(用户提供的"新蒸程 V1.5.9"),`--ignored` 手动运行。
//! 计时拆解:解析(MCI 补齐哈希)与下载提交两个阶段,输出吞吐供优化对比。
#![allow(missing_docs)]

use std::{fs, path::PathBuf, time::Instant};

use moyumax_core::{AppService, MciMirrorClient, parse_modpack_archive};
use rusqlite::{Connection, params};
use tempfile::TempDir;

const PACK_PATH: &str = "D:\\Downloads\\你好,新蒸程！V1.5.9.zip";

#[tokio::test]
#[ignore = "live:真实下载 650 MiB,基准计时"]
async fn live_modpack_download_bench() {
    let directory = TempDir::new().unwrap();
    // 基准环境本机代理路径已损坏(system-proxy 实测全灭),强制直连测真实吞吐。
    moyumax_core::set_active_proxy_preference(moyumax_core::ProxyPreference::Direct);
    let database_path = directory.path().join("state.sqlite3");
    let data_directory = directory.path().join("data");
    let instance_id = "bench-instance".to_owned();
    let instance_root = data_directory.join("instances").join(&instance_id);
    let service = AppService::open(&database_path, &data_directory).unwrap();
    service.skip_onboarding().unwrap();
    fs::create_dir_all(instance_root.join(".minecraft")).unwrap();
    Connection::open(&database_path)
        .unwrap()
        .execute(
            "INSERT INTO instances (
                id, name, game_version, loader_kind, loader_version,
                root_directory, state, created_at_unix_seconds
            ) VALUES (?1, '下载基准', '1.21.1', 'neoforge', '21.1.233', ?2, 'ready', 1)",
            params![instance_id, instance_root.to_string_lossy()],
        )
        .unwrap();

    let pack = PathBuf::from(PACK_PATH);
    assert!(pack.is_file(), "找不到基准整合包:{PACK_PATH}");
    let plan = parse_modpack_archive(&pack).unwrap();
    eprintln!(
        "plan: {} files, game {} loader {} {}",
        plan.files.len(),
        plan.game_version,
        plan.loader_kind,
        plan.loader_version
    );

    let started = Instant::now();
    let mci = MciMirrorClient::new().unwrap();
    let report = service
        .install_modpack_files(&plan, &pack, &instance_id, &mci, &|done, total, item| {
            if done % 25 == 0 || done == total {
                eprintln!("progress {done}/{total} {item} ({:?})", started.elapsed());
            }
        })
        .await
        .unwrap();
    let elapsed = started.elapsed();
    let installed_bytes: u64 = fs::read_dir(instance_root.join(".minecraft"))
        .map(|entries| {
            entries
                .filter_map(|entry| {
                    let path = entry.ok()?.path();
                    dir_size(&path)
                })
                .sum()
        })
        .unwrap_or(0);
    let mib = installed_bytes as f64 / 1024.0 / 1024.0;
    eprintln!(
        "== done: {} files, {mib:.1} MiB on disk, total {elapsed:?}, {:.2} MiB/s ==",
        report.installed_files,
        mib / elapsed.as_secs_f64(),
    );
}

fn dir_size(path: &std::path::Path) -> Option<u64> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.is_file() {
        return Some(metadata.len());
    }
    if !metadata.is_dir() {
        return None;
    }
    let mut total = 0;
    for entry in fs::read_dir(path).ok()?.flatten() {
        total += dir_size(&entry.path()).unwrap_or(0);
    }
    Some(total)
}
