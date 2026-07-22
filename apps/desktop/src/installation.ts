import type {
  FabricLoaderSummary,
  GameVersionSummary,
  InstallStage,
  LoaderChoice,
} from "./runtime";

export function recommendedVersion(
  versions: readonly GameVersionSummary[],
): GameVersionSummary | null {
  return versions.find((version) => version.recommended) ?? versions[0] ?? null;
}

export function recommendedFabricLoader(
  loaders: readonly FabricLoaderSummary[],
): FabricLoaderSummary | null {
  return loaders.find((loader) => loader.recommended) ?? loaders[0] ?? null;
}

export function defaultInstanceName(
  gameVersion: string,
  loader: LoaderChoice,
): string {
  return loader.kind === "fabric"
    ? `${gameVersion} Fabric`
    : `${gameVersion} 原版`;
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "未知";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

export function installStageLabel(stage: InstallStage): string {
  const labels: Record<InstallStage, string> = {
    prepare: "准备安装",
    downloadGameFiles: "下载游戏文件",
    verifyFiles: "验证文件",
    installGameEnvironment: "安装游戏环境",
    applyLoader: "应用加载器",
    commitChanges: "提交更改",
    createRollbackPoint: "创建回滚点",
  };
  return labels[stage];
}
