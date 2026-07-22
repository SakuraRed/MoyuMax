use moyumax_core::{
    InstallSelection, InstanceIsolation, LoaderChoice, MetadataClient, ResolvedLoader,
};

#[tokio::test]
#[ignore = "需要访问 Mojang、Fabric 与 Azul 官方元数据服务"]
async fn official_metadata_resolves_a_verified_default_install_request() {
    let client = MetadataClient::new().expect("metadata client should initialize");
    let catalog = client
        .fetch_version_catalog()
        .await
        .expect("Mojang version catalog should be available");
    let game_version = catalog
        .versions
        .iter()
        .find(|version| version.recommended)
        .expect("official catalog should mark a recommended release")
        .clone();
    let loader = client
        .compatible_fabric_loaders(&game_version.id)
        .await
        .expect("Fabric compatibility metadata should be available")
        .into_iter()
        .find(|candidate| candidate.recommended)
        .expect("Fabric should provide a recommended compatible loader");

    let request = client
        .resolve_install_request(&InstallSelection {
            instance_name: format!("{} Fabric", game_version.id),
            game_version,
            loader: LoaderChoice::Fabric {
                version: loader.version,
            },
            isolation: InstanceIsolation::Full,
        })
        .await
        .expect("game, Fabric and Azul metadata should resolve together");

    assert!(!request.game.artifacts.is_empty());
    assert!(request.game.asset_objects_total_bytes > 0);
    assert!(request.game.artifacts.iter().all(|artifact| {
        artifact
            .sha1
            .as_deref()
            .is_some_and(|value| !value.is_empty())
            || artifact
                .sha256
                .as_deref()
                .is_some_and(|value| !value.is_empty())
    }));
    assert!(matches!(request.loader, ResolvedLoader::Fabric { .. }));
    assert_eq!(
        request.java.artifact.sha256.as_deref().map(str::len),
        Some(64)
    );
    assert!(request.java.artifact.size > 0);
    let java_download = reqwest::Client::new()
        .head(&request.java.artifact.url)
        .send()
        .await
        .expect("Azul Java download should accept HEAD")
        .error_for_status()
        .expect("Azul Java download should be available");
    let download_size = java_download
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .expect("Azul Java download should declare Content-Length")
        .to_str()
        .expect("Content-Length should be ASCII")
        .parse::<u64>()
        .expect("Content-Length should be an integer");
    assert_eq!(
        download_size, request.java.artifact.size,
        "the immutable install snapshot should use the current download length"
    );
}
