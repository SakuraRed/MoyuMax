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
const cycles = Number(option("--cycles", "10"));
const runId = new Date().toISOString().replace(/[-:T]/g, "").slice(0, 14);
const outputDir = join(repoRoot, "output", "release-smoke");
const stateDir = join(outputDir, `state-${runId}`);
const tracePath = join(stateDir, "moyumax-smoke-trace.jsonl");
const resultPath = join(outputDir, `m9-smoke-${runId}.json`);

const WAKE_P95_BUDGET_MS = 250;
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

console.log(`启动 ${exePath}(MOYUMAX_SMOKE=1,cycles=${cycles})`);
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
const percentile = (values, ratio) =>
  values.length === 0 ? null : values[Math.min(values.length - 1, Math.ceil(values.length * ratio) - 1)];

// 采样分类直接使用采样时刻实时读取的阶段,避免事后按时间轴映射的偏移误差。
const bytesOf = (phase) =>
  samples.filter((sample) => sample.phase === phase).map((sample) => sample.bytes);
const destroyedBytes = bytesOf("hidden-destroyed");
const hiddenAliveBytes = bytesOf("hidden-alive");
const mib = (bytes) => Math.round((bytes / 1024 / 1024) * 10) / 10;

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
