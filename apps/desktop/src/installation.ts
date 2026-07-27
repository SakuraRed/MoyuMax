import { t } from "./i18n.svelte";
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
  if (loader.kind === "fabric") return `${gameVersion} Fabric`;
  if (loader.kind === "quilt") return `${gameVersion} Quilt`;
  if (loader.kind === "forge") return `${gameVersion} Forge`;
  if (loader.kind === "neoforge") return `${gameVersion} NeoForge`;
  return `${gameVersion} 原版`;
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return t("common.unknown");
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
  const keys: Record<InstallStage, string> = {
    prepare: "install.stage.prepare",
    downloadGameFiles: "install.stage.downloadGameFiles",
    verifyFiles: "install.stage.verifyFiles",
    installGameEnvironment: "install.stage.installGameEnvironment",
    applyLoader: "install.stage.applyLoader",
    commitChanges: "install.stage.commitChanges",
    createRollbackPoint: "install.stage.createRollbackPoint",
    modpackFiles: "install.stage.modpackFiles",
  };
  return t(keys[stage]);
}

/** 任务进度的无障碍摘要:已完成字节数加总量(未知时不伪造)。 */
export function taskProgressAriaLabel(progress: {
  completedBytes: number;
  totalBytes: number | null;
}): string {
  const completed = t("tasks.progress.completed").replace(
    "{completed}",
    String(progress.completedBytes),
  );
  return (
    completed +
    (progress.totalBytes === null
      ? t("tasks.progress.totalUnknown")
      : t("tasks.progress.totalKnown").replace("{total}", String(progress.totalBytes)))
  );
}
