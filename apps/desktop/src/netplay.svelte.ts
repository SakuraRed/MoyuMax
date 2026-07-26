// 联机房间全局状态：任何页面都能感知当前房间(悬浮窗/状态栏),
// 进程由桌面 NetplayCoordinator 持有,页面切换不影响组网生命周期。

import type { MoyuRuntime, NetplayRoomView } from "./runtime";

let room = $state<NetplayRoomView | null>(null);

export function netplayRoom(): NetplayRoomView | null {
  return room;
}

export function setNetplayRoom(view: NetplayRoomView | null): void {
  room = view;
}

/** 从桌面同步最新房间状态(含 DHCP 解析到的实际 IP)。 */
export async function refreshNetplayRoom(runtime: MoyuRuntime): Promise<void> {
  try {
    room = await runtime.getNetplayStatus();
  } catch {
    // 状态读取失败时保持现状
  }
}
