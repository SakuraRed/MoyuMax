// M9 托盘唤醒与后台内存冒烟驱动。
// 用法:node scripts/release-smoke.mjs [--exe <路径>] [--cycles N]
// 前置:已执行 `corepack pnpm --filter @moyumax/desktop build` 与
//      `cargo build --release -p moyumax-desktop`(或完整 tauri build)。
// 行为:以隔离状态目录启动 Release 进程并注入 MOYUMAX_SMOKE=1,应用自动执行
//      最小化→唤醒循环;本脚本采样进程树私有内存,结束后解析
//      moyumax-smoke-trace.jsonl,输出唤醒耗时与后台内存样本到 output/release-smoke/。
import { spawn, execFile } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const option = (name, fallback) => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : fallback;
};

const exePath = resolve(
  repoRoot,
  option("--exe", "target/release/moyumax-desktop.exe"),
);
const mode = option("--mode", "wake");
const cycles = Number(option("--cycles", mode === "cold" ? "3" : "10"));
const runId = new Date().toISOString().replace(/[-:T]/g, "").slice(0, 14);
const outputDir = join(repoRoot, "output", "release-smoke");
const stateDir = join(outputDir, `state-${runId}`);
const tracePath = join(stateDir, "moyumax-smoke-trace.jsonl");
const resultPath = join(outputDir, `m23-cold-${runId}.json`);

const WAKE_P95_BUDGET_MS = 250;
const COLD_VISIBLE_P95_BUDGET_MS = 500;
const COLD_INTERACTIVE_P50_BUDGET_MS = 1000;
const COLD_INTERACTIVE_P95_BUDGET_MS = 2000;
const FOREGROUND_TARGET_BYTES = 180 * 1024 * 1024;
const FOREGROUND_HARD_LIMIT_BYTES = 256 * 1024 * 1024;
const BACKGROUND_TARGET_BYTES = 80 * 1024 * 1024;
const BACKGROUND_HARD_LIMIT_BYTES = 120 * 1024 * 1024;

if (!existsSync(exePath)) {
  console.error(`找不到 Release 可执行文件:${exePath}`);
  console.error("请先构建前端并执行 cargo build --release -p moyumax-desktop");
  process.exit(2);
}
mkdirSync(stateDir, { recursive: true });

const samples = [];
let phase = "unknown";
const percentile = (values, ratio) =>
  values.length === 0 ? null : values[Math.min(values.length - 1, Math.ceil(values.length * ratio) - 1)];
const mib = (bytes) => Math.round((bytes / 1024 / 1024) * 10) / 10;

function readEvents(path) {
  return existsSync(path)
    ? readFileSync(path, "utf8")
        .trim()
        .split("\n")
        .filter(Boolean)
        .map((line) => JSON.parse(line))
    : [];
}

async function waitFor(predicate, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = predicate();
    if (value) return value;
    await new Promise((resolveSleep) => setTimeout(resolveSleep, 100));
  }
  throw new Error(`${label} 超时`);
}

// 冷启动模式:每轮以全新隔离状态目录启动 Release 进程,
// 记录进程启动 → 首个 window_shown(首窗可见)与 → 首个 bootstrap_ipc(可操作),
// 窗口展示期间采样一次前台进程树私有内存,结束后按预算判定。
async function runColdMode() {
  const runs = [];
  for (let cycle = 0; cycle < cycles; cycle += 1) {
    const coldStateDir = join(stateDir, `cold-${cycle}`);
    mkdirSync(coldStateDir, { recursive: true });
    const coldTrace = join(coldStateDir, "moyumax-smoke-trace.jsonl");
    const spawnedAt = Date.now();
    const child = spawn(exePath, [], {
      env: {
        ...process.env,
        MOYUMAX_STATE_DIR: coldStateDir,
        MOYUMAX_DATA_DIR: join(coldStateDir, "data"),
        MOYUMAX_SMOKE: "1",
        MOYUMAX_SMOKE_CYCLES: "1",
      },
      stdio: "ignore",
    });
    // trace 事件是应用侧相对毫秒,以 trace 文件出现时刻锚定到墙钟。
    const appearedAt = await waitFor(
      () => (existsSync(coldTrace) ? Date.now() : null),
      60_000,
      `第 ${cycle + 1} 轮冷启动 trace`,
    );
    const interactive = await waitFor(
      () => {
        const events = readEvents(coldTrace);
        const shown = events.find((entry) => entry.event === "window_shown");
        // bootstrap_call 是前端完成加载后的首个 IPC,即可操作时刻。
        const call = events.find((entry) => entry.event === "bootstrap_call");
        return shown && call ? { shown, call } : null;
      },
      60_000,
      `第 ${cycle + 1} 轮冷启动事件`,
    );
    const memory = await sampleOnce(child.pid);
    runs.push({
      cycle: cycle + 1,
      visibleMs: appearedAt + interactive.shown.ms - spawnedAt,
      interactiveMs: appearedAt + interactive.call.ms - spawnedAt,
      foregroundPrivateBytes: memory?.bytes ?? null,
      foregroundProcesses: memory?.processes ?? "",
    });
    child.kill();
    try {
      await execFileAsync("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], {
        timeout: 5000,
      });
    } catch {
      // 进程可能已随冒烟驱动自行退出。
    }
    await new Promise((resolveSleep) => setTimeout(resolveSleep, 500));
  }
  const visible = runs.map((run) => run.visibleMs).sort((left, right) => left - right);
  const interactive = runs.map((run) => run.interactiveMs).sort((left, right) => left - right);
  const foreground = runs
    .map((run) => run.foregroundPrivateBytes)
    .filter((value) => typeof value === "number");
  const result = {
    generatedAt: new Date().toISOString(),
    exePath,
    mode: "cold",
    cycles: runs,
    visibleMs: visible,
    interactiveMs: interactive,
    visibleP95Ms: percentile(visible, 0.95),
    interactiveP50Ms: percentile(interactive, 0.5),
    interactiveP95Ms: percentile(interactive, 0.95),
    foregroundPrivateBytes: {
      samples: foreground,
      max: foreground.length ? Math.max(...foreground) : null,
    },
    budgets: {
      visibleP95Ms: COLD_VISIBLE_P95_BUDGET_MS,
      interactiveP50Ms: COLD_INTERACTIVE_P50_BUDGET_MS,
      interactiveP95Ms: COLD_INTERACTIVE_P95_BUDGET_MS,
      foregroundTargetBytes: FOREGROUND_TARGET_BYTES,
      foregroundHardLimitBytes: FOREGROUND_HARD_LIMIT_BYTES,
    },
    checks: {
      visibleP95:
        visible.length > 0 && percentile(visible, 0.95) <= COLD_VISIBLE_P95_BUDGET_MS,
      interactiveP50:
        interactive.length > 0 && percentile(interactive, 0.5) <= COLD_INTERACTIVE_P50_BUDGET_MS,
      interactiveP95:
        interactive.length > 0 && percentile(interactive, 0.95) <= COLD_INTERACTIVE_P95_BUDGET_MS,
      foregroundHard:
        foreground.length > 0 && Math.max(...foreground) <= FOREGROUND_HARD_LIMIT_BYTES,
    },
    note: "首窗可见为进程启动到应用完成初始化(配置窗口已创建);可操作为首个 bootstrap_call(前端加载完成);前台口径为窗口展示期间进程树私有字节。",
  };
  writeFileSync(resultPath, `${JSON.stringify(result, null, 2)}\n`);
  console.log(`首窗可见样本(ms):${visible.join(", ")}(P95 预算 ${COLD_VISIBLE_P95_BUDGET_MS} ms)`);
  console.log(
    `可操作样本(ms):${interactive.join(", ")}(P50 ${result.interactiveP50Ms} / P95 ${result.interactiveP95Ms},预算 ${COLD_INTERACTIVE_P50_BUDGET_MS}/${COLD_INTERACTIVE_P95_BUDGET_MS} ms)`,
  );
  console.log(
    `前台私有内存峰值:${mib(result.foregroundPrivateBytes.max ?? 0)} MiB(目标 ${mib(FOREGROUND_TARGET_BYTES)} / 硬上限 ${mib(FOREGROUND_HARD_LIMIT_BYTES)} MiB)`,
  );
  console.log(`结果已写入 ${resultPath}`);
  const failed = Object.values(result.checks).some((check) => !check);
  if (failed) {
    console.error("冷启动预算未通过");
    process.exit(1);
  }
  process.exit(0);
}

async function sampleOnce(pid) {
  const script = [
    `$root=${pid};$ids=@($root);$i=0;`,
    "while($i -lt $ids.Count){$p=$ids[$i];$i++;",
    "Get-CimInstance Win32_Process -Filter \"ParentProcessId=$p\" -ErrorAction SilentlyContinue",
    " | ForEach-Object { $ids += $_.ProcessId }};",
    "$sum=0;$names=@();foreach($id in $ids){$pr=Get-Process -Id $id -ErrorAction SilentlyContinue;",
    "if($pr){$sum+=$pr.PrivateMemorySize64;$names+=$pr.ProcessName}};",
    "Write-Output (\"{0}|{1}\" -f $sum,($names -join ','))",
  ].join("");
  try {
    const { stdout } = await execFileAsync(
      "powershell.exe",
      ["-NoProfile", "-Command", script],
      { timeout: 5000 },
    );
    const [bytes, names] = stdout.trim().split("|");
    return {
      at: Date.now(),
      bytes: Number(bytes) || 0,
      processes: names ?? "",
      phase,
    };
  } catch {
    return null;
  }
}

console.log(`启动 ${exePath}(MOYUMAX_SMOKE=1,mode=${mode},cycles=${cycles})`);
if (mode !== "cold") {
const child = spawn(exePath, [], {
  env: {
    ...process.env,
    MOYUMAX_STATE_DIR: stateDir,
    MOYUMAX_DATA_DIR: join(stateDir, "data"),
    MOYUMAX_SMOKE: "1",
    MOYUMAX_SMOKE_CYCLES: String(cycles),
  },
  stdio: "ignore",
});

let childExited = false;
child.on("exit", () => {
  childExited = true;
});

const startedAt = Date.now();
while (!childExited && Date.now() - startedAt < 180_000) {
  if (existsSync(tracePath)) {
    const lines = readFileSync(tracePath, "utf8").trim().split("\n").filter(Boolean);
    const last = lines.length ? JSON.parse(lines[lines.length - 1]).event : "visible";
    phase =
      last === "window_hidden"
        ? "hidden-alive"
        : last === "window_destroyed"
          ? "hidden-destroyed"
          : last === "wake_trigger"
            ? "waking"
            : "visible";
  }
  const sample = await sampleOnce(child.pid);
  if (sample) samples.push(sample);
  await new Promise((resolveSleep) => setTimeout(resolveSleep, 250));
}
if (!childExited) {
  child.kill();
  console.error("冒烟序列超时,已终止进程");
  process.exit(2);
}

const events = existsSync(tracePath)
  ? readFileSync(tracePath, "utf8")
      .trim()
      .split("\n")
      .filter(Boolean)
      .map((line) => JSON.parse(line))
  : [];

// 快速唤醒:窗口隐藏但界面保留,wake_trigger → wake_shown。
// 慢速唤醒:WebView 已销毁,wake_trigger → 首个 bootstrap_ipc。
const fastWakes = [];
const slowWakes = [];
for (let index = 0; index < events.length; index += 1) {
  if (events[index].event !== "wake_trigger") continue;
  const rest = events.slice(index + 1);
  const nextTrigger = rest.findIndex((entry) => entry.event === "wake_trigger");
  const window_ = rest.slice(0, nextTrigger < 0 ? rest.length : nextTrigger);
  const shown = window_.find((entry) => entry.event === "wake_shown");
  if (shown) {
    fastWakes.push(shown.ms - events[index].ms);
    continue;
  }
  const ipc = window_.find((entry) => entry.event === "bootstrap_ipc");
  if (ipc && window_.some((entry) => entry.event === "wake_window_built")) {
    slowWakes.push(ipc.ms - events[index].ms);
  }
}
fastWakes.sort((left, right) => left - right);
slowWakes.sort((left, right) => left - right);

// 采样分类直接使用采样时刻实时读取的阶段,避免事后按时间轴映射的偏移误差。
const bytesOf = (phase) =>
  samples.filter((sample) => sample.phase === phase).map((sample) => sample.bytes);
const destroyedBytes = bytesOf("hidden-destroyed");
const hiddenAliveBytes = bytesOf("hidden-alive");

const result = {
  generatedAt: new Date().toISOString(),
  exePath,
  cycles,
  fastWakesMs: fastWakes,
  slowWakesMs: slowWakes,
  fastWakeP50Ms: percentile(fastWakes, 0.5),
  fastWakeP95Ms: percentile(fastWakes, 0.95),
  slowWakeP95Ms: percentile(slowWakes, 0.95),
  destroyedSamples: destroyedBytes.length,
  destroyedPrivateBytes: {
    average: destroyedBytes.length
      ? Math.round(destroyedBytes.reduce((sum, value) => sum + value, 0) / destroyedBytes.length)
      : null,
    max: destroyedBytes.length ? Math.max(...destroyedBytes) : null,
  },
  hiddenAlivePrivateBytes: {
    max: hiddenAliveBytes.length ? Math.max(...hiddenAliveBytes) : null,
    note: "隐藏保留界面期间的内存,只计入快速唤醒窗口期,不作为后台预算口径",
  },
  budgets: {
    fastWakeP95Ms: WAKE_P95_BUDGET_MS,
    backgroundTargetBytes: BACKGROUND_TARGET_BYTES,
    backgroundHardLimitBytes: BACKGROUND_HARD_LIMIT_BYTES,
  },
  checks: {
    fastWakeP95:
      fastWakes.length > 0 && percentile(fastWakes, 0.95) <= WAKE_P95_BUDGET_MS,
    backgroundHard:
      destroyedBytes.length > 0 &&
      Math.max(...destroyedBytes) <= BACKGROUND_HARD_LIMIT_BYTES,
    backgroundTarget:
      destroyedBytes.length > 0 &&
      Math.max(...destroyedBytes) <= BACKGROUND_TARGET_BYTES,
  },
  note: "快速唤醒为 wake_trigger 到窗口前置;慢速唤醒为 wake_trigger 到首个 bootstrap IPC;后台口径为 WebView 销毁后的进程树私有字节。",
};
writeFileSync(resultPath, `${JSON.stringify(result, null, 2)}\n`);

console.log(`快速唤醒样本(ms):${fastWakes.join(", ") || "无"}`);
console.log(`快速唤醒 P50/P95:${result.fastWakeP50Ms}/${result.fastWakeP95Ms} ms(预算 ${WAKE_P95_BUDGET_MS} ms)`);
console.log(`慢速唤醒样本(ms):${slowWakes.join(", ") || "无"}(WebView 重建,信息项)`);
console.log(
  `销毁后后台私有内存:平均 ${mib(result.destroyedPrivateBytes.average ?? 0)} MiB,峰值 ${mib(result.destroyedPrivateBytes.max ?? 0)} MiB(目标 ${mib(BACKGROUND_TARGET_BYTES)} / 硬上限 ${mib(BACKGROUND_HARD_LIMIT_BYTES)} MiB)`,
);
console.log(`结果已写入 ${resultPath}`);

if (!result.checks.fastWakeP95 || !result.checks.backgroundHard) {
  console.error("冒烟预算未通过");
  process.exit(1);
}
console.log(result.checks.backgroundTarget ? "冒烟通过(达到目标)" : "冒烟通过(未达目标,低于硬上限)");
}

if (mode === "cold") {
  await runColdMode();
}
