// 资源收藏夹：localStorage 持久化的 UI 偏好（与主题偏好同级，不写入核心库）。
// 桌面与浏览器测试环境通用；读写均做异常与损坏数据安全，失败时回退为空列表。

import type { ModrinthProjectType } from "./runtime";

export interface FavoriteProject {
  projectId: string;
  slug: string;
  title: string;
  iconUrl: string | null;
  type: ModrinthProjectType;
  addedAtUnixSeconds: number;
}

export type FavoriteProjectInput = Omit<FavoriteProject, "addedAtUnixSeconds">;

const FAVORITES_KEY = "moyumax.favorites";

const FAVORITE_TYPES: ModrinthProjectType[] = ["mod", "modpack", "shader", "resourcepack"];

function storage(): Storage | null {
  try {
    return typeof window === "undefined" ? null : window.localStorage;
  } catch {
    return null;
  }
}

function sanitize(value: unknown): FavoriteProject[] {
  if (!Array.isArray(value)) return [];
  const result: FavoriteProject[] = [];
  for (const candidate of value) {
    if (typeof candidate !== "object" || candidate === null) continue;
    const entry = candidate as Record<string, unknown>;
    if (
      typeof entry.projectId !== "string" ||
      typeof entry.slug !== "string" ||
      typeof entry.title !== "string" ||
      !FAVORITE_TYPES.includes(entry.type as ModrinthProjectType)
    ) {
      continue;
    }
    result.push({
      projectId: entry.projectId,
      slug: entry.slug,
      title: entry.title,
      iconUrl: typeof entry.iconUrl === "string" ? entry.iconUrl : null,
      type: entry.type as ModrinthProjectType,
      addedAtUnixSeconds:
        typeof entry.addedAtUnixSeconds === "number" ? entry.addedAtUnixSeconds : 0,
    });
  }
  return result;
}

function loadFavorites(): FavoriteProject[] {
  const store = storage();
  if (!store) return [];
  try {
    return sanitize(JSON.parse(store.getItem(FAVORITES_KEY) ?? "[]"));
  } catch {
    return [];
  }
}

let favorites = $state<FavoriteProject[]>(loadFavorites());

function persist(): void {
  try {
    storage()?.setItem(FAVORITES_KEY, JSON.stringify(favorites));
  } catch {
    // 存储不可用（隐私模式等）时保持内存态，不阻断交互。
  }
}

export function listFavorites(): FavoriteProject[] {
  return favorites;
}

export function isFavorite(projectId: string): boolean {
  return favorites.some((candidate) => candidate.projectId === projectId);
}

/** 切换收藏状态；返回切换后是否已收藏。 */
export function toggleFavorite(entry: FavoriteProjectInput): boolean {
  if (isFavorite(entry.projectId)) {
    favorites = favorites.filter((candidate) => candidate.projectId !== entry.projectId);
    persist();
    return false;
  }
  favorites = [
    { ...entry, addedAtUnixSeconds: Math.floor(Date.now() / 1000) },
    ...favorites,
  ];
  persist();
  return true;
}
