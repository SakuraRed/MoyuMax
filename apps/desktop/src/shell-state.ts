/**
 * 托盘唤醒恢复用到的壳层页面注册表。
 *
 * 恢复规则:
 * - 只有白名单内的页面可以恢复;未知页面一律回退首页。
 * - 敏感明文页面(账户、密码保险库等)即使出现在持久化状态中也回退首页。
 *   当前版本还没有敏感页面,该机制为账户里程碑预留并以单测固定。
 */

export type ShellPage = "home" | "instances" | "install" | "resources" | "netplay" | "tasks" | "data" | "backups" | "crash";

export interface ShellState {
  page: ShellPage;
  scrollTop: number;
}

const RESTORABLE_PAGES: ReadonlySet<string> = new Set<ShellPage>([
  "home",
  "instances",
  "install",
  "resources",
  "netplay",
  "tasks",
  "data",
  "backups",
  "crash",
]);

/** 敏感明文页面注册表:唤醒恢复时不得回到这些页面,必须回到首页。 */
const SENSITIVE_PAGES: ReadonlySet<string> = new Set<string>([]);

export function isSensitivePage(page: string): boolean {
  return SENSITIVE_PAGES.has(page);
}

export function isRestorablePage(page: string): page is ShellPage {
  return RESTORABLE_PAGES.has(page) && !SENSITIVE_PAGES.has(page);
}

/**
 * 校验并清洗持久化的壳层状态。
 * 返回 null 表示调用方应回退到首页(未知页面、敏感页面或数据损坏)。
 * `options.sensitivePages` 供测试注入敏感页面集合,生产调用使用全局注册表。
 */
export function sanitizeShellState(
  raw: unknown,
  options?: { sensitivePages?: ReadonlySet<string> },
): ShellState | null {
  if (typeof raw !== "object" || raw === null) return null;
  const candidate = raw as { page?: unknown; scrollTop?: unknown };
  if (typeof candidate.page !== "string") return null;
  const sensitive = options?.sensitivePages ?? SENSITIVE_PAGES;
  if (sensitive.has(candidate.page)) return null;
  if (!RESTORABLE_PAGES.has(candidate.page)) return null;
  const scrollTop =
    typeof candidate.scrollTop === "number" && Number.isFinite(candidate.scrollTop)
      ? Math.max(0, Math.round(candidate.scrollTop))
      : 0;
  return { page: candidate.page as ShellPage, scrollTop };
}
