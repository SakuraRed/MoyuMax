/** 全局搜索(Ctrl+K / 标题栏搜索入口)的开关状态,挂在 App 层读取本地数据。 */

let open = $state(false);

export function globalSearchOpen(): boolean {
  return open;
}

export function openGlobalSearch(): void {
  open = true;
}

export function closeGlobalSearch(): void {
  open = false;
}

export function toggleGlobalSearch(): void {
  open = !open;
}
