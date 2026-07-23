/**
 * 关闭窗口决策:首次询问、记住行为与退出影响检查的纯逻辑。
 */

import type { ExitImpact, WindowCloseBehavior } from "./runtime";

export type { WindowCloseBehavior };

export type CloseRoute = "choice-dialog" | "impact-dialog" | "minimize" | "exit";

/** 是否需要先向用户说明退出影响。已暂停任务不阻塞退出。 */
export function impactRequiresConfirmation(impact: ExitImpact): boolean {
  return (
    impact.runningSessions.length > 0 ||
    impact.activeInstallTasks > 0 ||
    impact.activeContentTasks > 0
  );
}

/**
 * 根据记住的关闭行为与当前退出影响决定关闭路径:
 * - ask:打开首次关闭选择对话框(对话框内展示影响摘要)。
 * - minimizeToTray:直接最小化到托盘。
 * - exit:无影响直接退出;有影响打开退出确认对话框。
 */
export function routeCloseRequest(
  behavior: WindowCloseBehavior,
  impact: ExitImpact,
): CloseRoute {
  if (behavior === "ask") return "choice-dialog";
  if (behavior === "minimizeToTray") return "minimize";
  return impactRequiresConfirmation(impact) ? "impact-dialog" : "exit";
}

export interface CloseDialogImpactLine {
  text: string;
  danger: boolean;
}

/** 退出确认对话框的影响清单(与托盘、对话框共用同一事实)。 */
export function describeExitImpact(impact: ExitImpact): CloseDialogImpactLine[] {
  const lines: CloseDialogImpactLine[] = [];
  for (const session of impact.runningSessions) {
    lines.push({
      text: `「${session.instanceName}」正在运行，退出将安全终止游戏；存档会在终止前自动完成一次退出备份。`,
      danger: true,
    });
  }
  const activeTasks = impact.activeInstallTasks + impact.activeContentTasks;
  if (activeTasks > 0) {
    lines.push({
      text: `${activeTasks} 个任务正在进行或排队，退出后可在下次启动时继续或恢复。`,
      danger: false,
    });
  }
  if (impact.pausedTasks > 0) {
    lines.push({
      text: `${impact.pausedTasks} 个任务已暂停，下次启动后可从任务中心恢复。`,
      danger: false,
    });
  }
  return lines;
}
