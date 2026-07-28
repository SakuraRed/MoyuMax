/**
 * 全局轻量通知(mockups 10 窗口 1):右上角堆叠、自动消失、可键盘访问、aria-live 播报。
 * Toast 只承担轻量成功反馈、撤销入口与非关键变化;持续状态用 Banner,风险确认用 Modal。
 */

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface ToastOptions {
  tone?: "ok" | "warn" | "danger" | "info";
  title: string;
  sub?: string;
  action?: ToastAction;
  /** 自动消失时长;0 表示常驻(需手动 dismiss)。 */
  durationMs?: number;
}

export interface ToastItem extends Required<Omit<ToastOptions, "sub" | "action" | "durationMs">> {
  id: number;
  sub: string;
  action: ToastAction | null;
  durationMs: number;
}

let nextId = 1;
let items = $state<ToastItem[]>([]);
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export function toastItems(): ToastItem[] {
  return items;
}

export function pushToast(options: ToastOptions): number {
  const id = nextId++;
  const item: ToastItem = {
    id,
    tone: options.tone ?? "ok",
    title: options.title,
    sub: options.sub ?? "",
    action: options.action ?? null,
    durationMs: options.durationMs ?? 4000,
  };
  items = [...items.slice(-4), item];
  if (item.durationMs > 0) {
    timers.set(
      id,
      setTimeout(() => dismissToast(id), item.durationMs),
    );
  }
  return id;
}

export function dismissToast(id: number): void {
  const timer = timers.get(id);
  if (timer) {
    clearTimeout(timer);
    timers.delete(id);
  }
  items = items.filter((item) => item.id !== id);
}

/** 测试与页面卸载时清空。 */
export function clearToasts(): void {
  for (const timer of timers.values()) clearTimeout(timer);
  timers.clear();
  items = [];
}
