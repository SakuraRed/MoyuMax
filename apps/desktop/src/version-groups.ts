/**
 * 在线资源版本分组(对齐 PCL-CE 的资源详情逻辑):
 * - 按 MC 精确版本分组(1.21.1 / 1.20.1),组内按发布日期降序,组按 MC 版本降序。
 * - 模组项目涉及多个加载器时,按「加载器 × 精确版本」复合分组(Fabric 1.21.1)。
 * - 整合包版本强制只归属其声明中最高的一个 MC 版本。
 * - 快照(24w14a)与无法识别的版本归入特殊组沉底。
 * - 与目标实例精确匹配的组置顶并标记推荐。
 */

import type { ModrinthVersionSummary } from "./runtime";

export type VersionGroupKind = "mod" | "modpack" | "shader" | "resourcepack";

export interface VersionGroupTarget {
  gameVersion: string;
  loaderKind: string;
}

export interface VersionGroup {
  key: string;
  label: string;
  recommended: boolean;
  versions: ModrinthVersionSummary[];
}

export const SNAPSHOT_GROUP_KEY = "__snapshot__";
export const UNKNOWN_GROUP_KEY = "__unknown__";

const SNAPSHOT_PATTERN = /^\d{2}w\d{2}[a-z]$/;

/** 标准 MC 版本:1.21.1 / 1.20 / b1.7.3(含小数字段)。 */
function isStandardGameVersion(value: string): boolean {
  return /^b?\d+\.\d+/.test(value);
}

/**
 * 游戏版本归一化:带预发布后缀的版本号(1.21.4-rc1 / 1.21-pre2)归入对应正式版本,
 * 不作为独立版本出现在分组、筛选与区间标签中。
 */
export function normalizeGameVersion(gameVersion: string): string {
  const dash = gameVersion.indexOf("-");
  return dash > 0 ? gameVersion.slice(0, dash) : gameVersion;
}

/** 版本号是否带预发布后缀(rc/pre/snapshot 等)。 */
export function isPrereleaseGameVersion(gameVersion: string): boolean {
  return gameVersion.includes("-");
}

export function compareGameVersionsDescending(left: string, right: string): number {
  const leftParts = left.split(".");
  const rightParts = right.split(".");
  const length = Math.max(leftParts.length, rightParts.length);
  for (let index = 0; index < length; index += 1) {
    const a = leftParts[index] ?? "";
    const b = rightParts[index] ?? "";
    const aNumber = Number(a);
    const bNumber = Number(b);
    if (a === b) continue;
    if (!Number.isNaN(aNumber) && !Number.isNaN(bNumber) && aNumber !== bNumber) {
      return bNumber - aNumber;
    }
    if (!Number.isNaN(aNumber)) return -1;
    if (!Number.isNaN(bNumber)) return 1;
    return b.localeCompare(a);
  }
  return 0;
}

function byDateDescending(left: ModrinthVersionSummary, right: ModrinthVersionSummary): number {
  return right.datePublished.localeCompare(left.datePublished);
}

/** 版本声明中最高的一个 MC 版本(整合包强制唯一归属);无标准版本时返回 null。 */
export function primaryGameVersion(version: ModrinthVersionSummary): string | null {
  const standards = version.gameVersions
    .filter(isStandardGameVersion)
    .map(normalizeGameVersion);
  if (standards.length === 0) return null;
  return [...standards].sort(compareGameVersionsDescending)[0] ?? null;
}

function specialGroupKey(version: ModrinthVersionSummary): string {
  return version.gameVersions.some((candidate) => SNAPSHOT_PATTERN.test(candidate))
    ? SNAPSHOT_GROUP_KEY
    : UNKNOWN_GROUP_KEY;
}

export function buildVersionGroups(
  versions: ModrinthVersionSummary[],
  options: { kind: VersionGroupKind; target?: VersionGroupTarget | null; collapseLoaders?: boolean },
): VersionGroup[] {
  const { kind, target = null, collapseLoaders = false } = options;
  const buckets = new Map<string, ModrinthVersionSummary[]>();
  const push = (key: string, version: ModrinthVersionSummary): void => {
    const bucket = buckets.get(key) ?? [];
    bucket.push(version);
    buckets.set(key, bucket);
  };

  // 模组且项目跨多个加载器时,分组键加加载器前缀(加载器 × 精确版本);
  // 已按单个加载器过滤时回到纯版本分组,避免冗余前缀。
  const allLoaders = [
    ...new Set(versions.flatMap((version) => version.loaders)),
  ].sort();
  const compoundLoader = kind === "mod" && allLoaders.length > 1 && !collapseLoaders;

  for (const version of versions) {
    if (kind === "modpack") {
      const primary = primaryGameVersion(version);
      push(primary ?? specialGroupKey(version), version);
      continue;
    }
    const standards = [
      ...new Set(
        version.gameVersions.filter(isStandardGameVersion).map(normalizeGameVersion),
      ),
    ];
    if (standards.length === 0) {
      push(specialGroupKey(version), version);
      continue;
    }
    const loaders = compoundLoader ? version.loaders : [""];
    for (const gameVersion of standards) {
      for (const loader of loaders) {
        push(compoundLoader ? `${loader} ${gameVersion}` : gameVersion, version);
      }
    }
  }

  const isRecommended = (key: string): boolean => {
    if (!target || kind === "modpack") return false;
    if (compoundLoader) {
      return key === `${target.loaderKind} ${target.gameVersion}`;
    }
    return key === target.gameVersion;
  };

  return [...buckets.entries()]
    .sort((a, b) => {
      const aSpecial = a[0] === SNAPSHOT_GROUP_KEY || a[0] === UNKNOWN_GROUP_KEY;
      const bSpecial = b[0] === SNAPSHOT_GROUP_KEY || b[0] === UNKNOWN_GROUP_KEY;
      if (aSpecial !== bSpecial) return aSpecial ? 1 : -1;
      if (aSpecial) return a[0] === UNKNOWN_GROUP_KEY ? 1 : -1;
      const aVersion = compoundLoader ? a[0].split(" ").pop() ?? a[0] : a[0];
      const bVersion = compoundLoader ? b[0].split(" ").pop() ?? b[0] : b[0];
      const byVersion = compareGameVersionsDescending(aVersion, bVersion);
      if (byVersion !== 0) return byVersion;
      return a[0].localeCompare(b[0]);
    })
    .map(([key, groupVersions]) => ({
      key,
      label: key,
      recommended: isRecommended(key),
      versions: [...groupVersions].sort(byDateDescending),
    }))
    .sort((a, b) => Number(b.recommended) - Number(a.recommended));
}

/** 版本选择器 option 文本:版本号 + 非 release 类型标注。 */
export function versionOptionLabel(version: ModrinthVersionSummary): string {
  return version.versionType !== "release"
    ? `${version.versionNumber} (${version.versionType})`
    : version.versionNumber;
}

/** 文件行的完整游戏版本标签(涵盖该版本支持的全部 MC 版本,顿号分隔)。 */
export function versionGameTags(version: ModrinthVersionSummary): string {
  return [...version.gameVersions].sort(compareGameVersionsDescending).join("、");
}

/** 大版本:仅对含两段以上的精确版本忽略最后一个 `.` 后的数字(1.20.1→1.20);
 * 两段版本本身即大版本(1.21→1.21),不能再砍。 */
function majorOf(gameVersion: string): string {
  const first = gameVersion.indexOf(".");
  const last = gameVersion.lastIndexOf(".");
  return last > first ? gameVersion.slice(0, last) : gameVersion;
}

/** 两个大版本是否相邻递增(1.12→1.13,1.19→1.20,25→26;1.16→26 不相邻)。 */
function isNextMajor(current: string, next: string): boolean {
  const currentParts = current.split(".");
  const nextParts = next.split(".");
  if (currentParts.length !== nextParts.length) return false;
  for (let index = 0; index < currentParts.length - 1; index += 1) {
    if (currentParts[index] !== nextParts[index]) return false;
  }
  return Number(nextParts[nextParts.length - 1]) === Number(currentParts[currentParts.length - 1]) + 1;
}

/**
 * 资源版本范围标签:连续大版本合并为区间,断档另起段。
 * 例:[1.12.2,1.13.2,1.14.4,1.16.5,26.2] → "1.12.2-1.16.5,26.2";
 * [26.1,26.2] → "26.1-26.2";[26.2] → "26.2"。非标准版本(快照等)忽略。
 */
export function formatGameVersionRange(versions: string[]): string {
  const standards = [
    ...new Set(
      versions
        .filter((candidate) => isStandardGameVersion(candidate))
        .map(normalizeGameVersion),
    ),
  ]
    .sort(compareGameVersionsDescending)
    .reverse();
  if (standards.length === 0) return versions[versions.length - 1] ?? "";

  interface MajorRun {
    major: string;
    min: string;
    max: string;
  }
  const majors: MajorRun[] = [];
  for (const gameVersion of standards) {
    const major = majorOf(gameVersion);
    const last = majors[majors.length - 1];
    if (last && last.major === major) {
      last.max = gameVersion;
    } else {
      majors.push({ major, min: gameVersion, max: gameVersion });
    }
  }

  const runs: { first: MajorRun; last: MajorRun }[] = [];
  for (const entry of majors) {
    const last = runs[runs.length - 1];
    if (last && isNextMajor(last.last.major, entry.major)) {
      last.last = entry;
    } else {
      runs.push({ first: entry, last: entry });
    }
  }

  return runs
    .map((run) =>
      run.first === run.last
        ? run.first.min === run.first.max
          ? run.first.min
          : `${run.first.min}-${run.first.max}`
        : `${run.first.min}-${run.last.max}`,
    )
    .join(",");
}
