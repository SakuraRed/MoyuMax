//! 全加载器实机矩阵:对全部声明支持的加载器(vanilla/fabric/quilt/forge/neoforge)
//! 在同一受管数据目录依次完成 安装 → 启动进入 Minecraft → 安全停止。
//! 共享存储复用,仅首个实例全量下载;需要网络,默认忽略,实机跑法:
//!   MOYUMAX_LIVE_MATRIX_DIR=D:\MoyuMax\live-matrix cargo test -p moyumax-core --test live_loader_matrix -- --ignored --nocapture

use moyumax_core::{
    AppService, InstallExecutor, InstallSelection, InstanceIsolation, LaunchAccount, LaunchOptions,
    LoaderChoice, MetadataClient, run_launch_execution,
};

const STOPPED_STATE: &str = "stopped";

const GAME_VERSION: &str = "1.21.1";

struct LoaderCase {
    kind: &'static str,
    markers: &'static [&'static str],
}

#[tokio::test]
#[ignore = "实机安装并启动全部加载器,耗时长且需要网络"]
async fn all_supported_loaders_install_and_launch_into_minecraft() {
    let root = std::env::var_os("MOYUMAX_LIVE_MATRIX_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("moyumax-loader-matrix"));
    std::fs::create_dir_all(&root).unwrap();
    let database = root.join("state.sqlite3");
    let data = root.join("data");
    let service = AppService::open(&database, &data).unwrap();
    service.skip_onboarding().unwrap();

    let metadata = MetadataClient::new().unwrap();
    let catalog = metadata.fetch_version_catalog().await.unwrap();
    service.store_version_catalog(&catalog).unwrap();
    let game_version = catalog
        .versions
        .iter()
        .find(|version| version.id == GAME_VERSION)
        .unwrap_or_else(|| panic!("版本清单缺少 {GAME_VERSION}"))
        .clone();

    let fabric = recommended(
        metadata
            .compatible_fabric_loaders(GAME_VERSION)
            .await
            .unwrap(),
        "fabric",
    );
    let quilt = recommended(
        metadata
            .compatible_quilt_loaders(GAME_VERSION)
            .await
            .unwrap(),
        "quilt",
    );
    let forge = recommended(
        metadata
            .compatible_forge_versions(GAME_VERSION)
            .await
            .unwrap(),
        "forge",
    );
    let neoforge = recommended(
        metadata
            .compatible_neoforge_versions(GAME_VERSION)
            .await
            .unwrap(),
        "neoforge",
    );

    let cases: Vec<(LoaderCase, LoaderChoice)> = vec![
        (
            LoaderCase {
                kind: "vanilla",
                markers: &["Backend library: LWJGL", "Setting user:"],
            },
            LoaderChoice::Vanilla,
        ),
        (
            LoaderCase {
                kind: "fabric",
                markers: &["Fabric Loader"],
            },
            LoaderChoice::Fabric { version: fabric },
        ),
        (
            LoaderCase {
                kind: "quilt",
                markers: &["Quilt Loader", "quilt_loader"],
            },
            LoaderChoice::Quilt { version: quilt },
        ),
        (
            LoaderCase {
                kind: "forge",
                markers: &["Backend library: LWJGL", "net.minecraftforge"],
            },
            LoaderChoice::Forge { version: forge },
        ),
        (
            LoaderCase {
                kind: "neoforge",
                markers: &["Backend library: LWJGL", "neoforge", "FML"],
            },
            LoaderChoice::NeoForge { version: neoforge },
        ),
    ];

    let mut summary = Vec::new();
    for (case, loader) in cases {
        let outcome = run_case(&service, &metadata, &game_version, &case, loader).await;
        summary.push(outcome);
    }

    let failures: Vec<_> = summary.iter().filter(|outcome| !outcome.success).collect();
    for outcome in &summary {
        println!(
            "[matrix] {} => install={} launch={} note={}",
            outcome.kind, outcome.installed, outcome.launched, outcome.note
        );
    }
    if let Some(result_file) = std::env::var_os("MOYUMAX_LIVE_RESULT_FILE") {
        std::fs::write(
            std::path::PathBuf::from(result_file),
            serde_json::to_vec_pretty(&serde_json::json!(
                summary
                    .iter()
                    .map(|o| {
                        serde_json::json!({
                            "loader": o.kind,
                            "installed": o.installed,
                            "launched": o.launched,
                            "note": o.note,
                        })
                    })
                    .collect::<Vec<_>>()
            ))
            .unwrap(),
        )
        .unwrap();
    }
    assert!(
        failures.is_empty(),
        "以下加载器未通过安装启动矩阵:{failures:?}"
    );
}

#[derive(Debug)]
struct CaseOutcome {
    kind: &'static str,
    installed: bool,
    launched: bool,
    success: bool,
    note: String,
}

fn recommended(list: Vec<moyumax_core::FabricLoaderSummary>, label: &str) -> String {
    list.iter()
        .find(|entry| entry.recommended)
        .or(list.first())
        .unwrap_or_else(|| panic!("{label} 没有可用版本"))
        .version
        .clone()
}

async fn run_case(
    service: &AppService,
    metadata: &MetadataClient,
    game_version: &moyumax_core::GameVersionSummary,
    case: &LoaderCase,
    loader: LoaderChoice,
) -> CaseOutcome {
    let kind = case.kind;
    let request = match metadata
        .resolve_install_request(&InstallSelection {
            instance_name: format!("矩阵 {kind} {GAME_VERSION}"),
            game_version: game_version.clone(),
            loader,
            isolation: InstanceIsolation::Full,
        })
        .await
    {
        Ok(request) => request,
        Err(error) => {
            return CaseOutcome {
                kind,
                installed: false,
                launched: false,
                success: false,
                note: format!("安装请求解析失败：{error}"),
            };
        }
    };
    let task = service.enqueue_install_task(&request).unwrap();
    let instance = match InstallExecutor::new(8)
        .unwrap()
        .execute_task(service, &task.id)
        .await
    {
        Ok(instance) => instance,
        Err(error) => {
            return CaseOutcome {
                kind,
                installed: false,
                launched: false,
                success: false,
                note: format!("安装失败：{error}"),
            };
        }
    };
    if instance.state != "ready" {
        return CaseOutcome {
            kind,
            installed: false,
            launched: false,
            success: false,
            note: format!("实例状态异常：{}", instance.state),
        };
    }

    let execution = service
        .create_launch_execution(
            &instance.id,
            &LaunchAccount::offline("MoyuMaxMatrix").unwrap(),
            &LaunchOptions {
                minimum_memory_mib: 512,
                maximum_memory_mib: 2_048,
            },
        )
        .unwrap();
    let stdout_path = std::path::PathBuf::from(&execution.session().stdout_path);
    let stderr_path = std::path::PathBuf::from(&execution.session().stderr_path);
    let markers: Vec<String> = case.markers.iter().map(|m| m.to_lowercase()).collect();
    let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel();
    let monitor = tokio::spawn(async move {
        for _ in 0..300 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let stdout = tokio::fs::read_to_string(&stdout_path)
                .await
                .unwrap_or_default();
            let stderr = tokio::fs::read_to_string(&stderr_path)
                .await
                .unwrap_or_default();
            let output = format!("{stdout}\n{stderr}").to_lowercase();
            if markers.iter().any(|marker| output.contains(marker)) {
                let _ = stop_sender.send(());
                return true;
            }
        }
        let _ = stop_sender.send(());
        false
    });
    let completed = run_launch_execution(service, execution, stop_receiver)
        .await
        .unwrap();
    let reached = monitor.await.unwrap();
    let no_crash = service
        .list_crash_reports()
        .unwrap()
        .iter()
        .all(|report| report.launch_session_id != completed.id);
    let clean_exit = format!("{:?}", completed.state)
        .to_lowercase()
        .contains(STOPPED_STATE);
    CaseOutcome {
        kind,
        installed: true,
        launched: reached,
        success: reached && no_crash && clean_exit,
        note: format!(
            "loader={} 进入游戏={} 无崩溃报告={} 正常停止={}",
            instance.loader_version.unwrap_or_default(),
            reached,
            no_crash,
            clean_exit
        ),
    }
}
