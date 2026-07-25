// 壳层账户状态：侧边栏左下角的默认账户显示（皮肤头像 + 用户名 + 登录方式）。
// 模块级状态由 App 启动与各账户操作后刷新；AppShell 直接订阅。

import type { AccountKind, MoyuRuntime } from "./runtime";

let name = $state("");
let kind = $state<AccountKind | null>(null);
let playerUuid = $state("");
let loaded = $state(false);
let avatarFailed = $state(false);

export interface ShellAccount {
  name: string;
  kind: AccountKind | null;
  playerUuid: string;
  loaded: boolean;
  avatarFailed: boolean;
}

export function shellAccount(): ShellAccount {
  return { name, kind, playerUuid, loaded, avatarFailed };
}

export function markAvatarFailed(): void {
  avatarFailed = true;
}

/** 刷新默认账户显示；任何账户变更（登录/设默认/移除/刷新会话）后调用。 */
export async function refreshShellAccount(runtime: MoyuRuntime): Promise<void> {
  try {
    const accounts = await runtime.listAccounts();
    const defaultAccount = accounts.find((account) => account.isDefault) ?? accounts[0];
    name = defaultAccount?.username ?? "";
    kind = defaultAccount?.kind ?? null;
    playerUuid = defaultAccount?.playerUuid ?? "";
    avatarFailed = false;
  } catch {
    name = "";
    kind = null;
    playerUuid = "";
  }
  loaded = true;
}

/** Minotar 皮肤头像（正版 UUID,helm 变体含双层皮肤外层,透明背景);
 * 离线账户返回空串由调用方回退字母头像。 */
export function skinAvatarUrl(uuid: string, accountKind: AccountKind | null): string {
  if (!uuid || accountKind !== "microsoft") return "";
  return `https://minotar.net/helm/${uuid.replaceAll("-", "")}/64.png`;
}

let requestedSettingsPage = $state<string | null>(null);

/** 请求设置页打开指定子页（如左下角账户按钮直达账户子页）。 */
export function requestSettingsPage(page: string): void {
  requestedSettingsPage = page;
}

/** 设置页挂载时消费一次性的子页请求。 */
export function consumeSettingsPage(): string | null {
  const page = requestedSettingsPage;
  requestedSettingsPage = null;
  return page;
}
