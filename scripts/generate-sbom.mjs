// MoyuMax SBOM 与第三方许可清单生成（可复现）。
// 用法:node scripts/generate-sbom.mjs [--check]
// 数据源:Cargo.lock、cargo metadata（许可证字段）、pnpm-lock.yaml、node_modules 包清单。
// 产物:docs/SBOM.json（CycloneDX 1.5 简式）与 docs/THIRD-PARTY-LICENSES.md。
// --check:不落盘,校验已提交产物与当前锁文件一致,并执行许可黑名单扫描。
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sbomPath = join(repoRoot, "docs", "SBOM.json");
const licensesPath = join(repoRoot, "docs", "THIRD-PARTY-LICENSES.md");
const checkOnly = process.argv.includes("--check");

// GPL-3.0-only 客户端不兼容或不接受的红线许可证。
const LICENSE_BLACKLIST = [
  /^AGPL/i,
  /^SSPL/i,
  /^GPL-2\.0-only/i,
  /^LicenseRef-Proprietary/i,
  /^CC-BY-NC/i,
  /^UNLICENSED$/i,
];

function parseCargoLock() {
  const text = readFileSync(join(repoRoot, "Cargo.lock"), "utf8");
  const packages = [];
  const pattern = /\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"/g;
  let match;
  while ((match = pattern.exec(text)) !== null) {
    packages.push({ name: match[1], version: match[2] });
  }
  return packages;
}

function cargoLicenses() {
  const output = execFileSync(
    "cargo",
    ["metadata", "--format-version", "1", "--locked"],
    { cwd: repoRoot, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
  );
  const metadata = JSON.parse(output);
  const map = new Map();
  for (const pkg of metadata.packages) {
    map.set(`${pkg.name}@${pkg.version}`, {
      license: pkg.license ?? "未知",
      repository: pkg.repository ?? "",
    });
  }
  return map;
}

function parsePnpmLock() {
  const text = readFileSync(join(repoRoot, "pnpm-lock.yaml"), "utf8");
  const packages = new Map();
  const pattern = /^\s{2}([a-zA-Z0-9@/._-]+)@([0-9][0-9A-Za-z.+-]*):$/gm;
  let match;
  while ((match = pattern.exec(text)) !== null) {
    packages.set(`${match[1]}@${match[2]}`, { name: match[1], version: match[2] });
  }
  return [...packages.values()];
}

function readDirNames(directory) {
  return readdirSync(directory, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name);
}

function npmLicense(name, version) {
  const pnpmDir = join(repoRoot, "node_modules", ".pnpm");
  const prefix = `${name.replace("/", "+")}@${version}`;
  const candidates = existsSync(pnpmDir)
    ? [prefix, ...readDirNames(pnpmDir).filter((entry) => entry.startsWith(`${prefix}(`))]
    : [];
  for (const candidate of candidates) {
    const path = join(pnpmDir, candidate, "node_modules", name, "package.json");
    if (!existsSync(path)) continue;
    const manifest = JSON.parse(readFileSync(path, "utf8"));
    const repository =
      typeof manifest.repository === "string"
        ? manifest.repository
        : (manifest.repository?.url ?? "");
    return { license: manifest.license ?? "未知", repository };
  }
  return { license: "未知（平台可选，未在本机安装）", repository: "" };
}

function collectComponents() {
  const cargoMeta = cargoLicenses();
  const components = [];
  for (const pkg of parseCargoLock()) {
    const meta = cargoMeta.get(`${pkg.name}@${pkg.version}`) ?? {
      license: "未知",
      repository: "",
    };
    components.push({
      type: "library",
      ecosystem: "cargo",
      name: pkg.name,
      version: pkg.version,
      purl: `pkg:cargo/${pkg.name}@${pkg.version}`,
      license: meta.license,
      repository: meta.repository,
    });
  }
  for (const pkg of parsePnpmLock()) {
    const meta = npmLicense(pkg.name, pkg.version);
    components.push({
      type: "library",
      ecosystem: "npm",
      name: pkg.name,
      version: pkg.version,
      purl: `pkg:npm/${pkg.name.replace("/", "%2F")}@${pkg.version}`,
      license: meta.license,
      repository: meta.repository,
    });
  }
  components.sort((left, right) =>
    left.ecosystem === right.ecosystem
      ? left.name.localeCompare(right.name)
      : left.ecosystem.localeCompare(right.ecosystem),
  );
  return components;
}

function renderSbom(components) {
  return {
    bomFormat: "CycloneDX",
    specVersion: "1.5",
    serialNumber: `urn:uuid:00000000-0000-4000-8000-moyumaxsbom0`,
    version: 1,
    metadata: {
      component: {
        type: "application",
        name: "MoyuMax",
        version: "0.1.0",
        licenses: ["GPL-3.0-only"],
      },
    },
    components,
  };
}

function renderLicensesMarkdown(components) {
  const lines = [
    "# MoyuMax 第三方许可清单",
    "",
    "本文件由 `node scripts/generate-sbom.mjs` 生成,与 Cargo.lock、pnpm-lock.yaml 保持一致。",
    "MoyuMax 客户端许可证为 GPL-3.0-only;下表列出全部直接及传递依赖的许可证。",
    "",
    "| 生态 | 包 | 版本 | 许可证 |",
    "|---|---|---|---|",
  ];
  for (const component of components) {
    lines.push(
      `| ${component.ecosystem} | ${component.name} | ${component.version} | ${component.license} |`,
    );
  }
  lines.push("");
  return lines.join("\n");
}

function scanBlacklist(components) {
  const violations = components.filter((component) =>
    LICENSE_BLACKLIST.some((pattern) => pattern.test(component.license)),
  );
  const unknown = components.filter((component) => component.license === "未知");
  if (unknown.length > 0) {
    console.error(
      `${unknown.length} 个依赖缺少许可证字段:${unknown.map((c) => `${c.ecosystem}/${c.name}`).join(", ")}`,
    );
    process.exit(1);
  }
  if (violations.length > 0) {
    console.error("许可黑名单命中:");
    for (const component of violations) {
      console.error(`  ${component.ecosystem}/${component.name}@${component.version} ${component.license}`);
    }
    process.exit(1);
  }
}

const components = collectComponents();
const sbom = `${JSON.stringify(renderSbom(components), null, 2)}\n`;
const markdown = renderLicensesMarkdown(components);

if (checkOnly) {
  const drift =
    !existsSync(sbomPath) ||
    !existsSync(licensesPath) ||
    readFileSync(sbomPath, "utf8") !== sbom ||
    readFileSync(licensesPath, "utf8") !== markdown;
  if (drift) {
    console.error("SBOM/许可清单与锁文件不一致,请重新运行 node scripts/generate-sbom.mjs");
    process.exit(1);
  }
  scanBlacklist(components);
  console.log(`SBOM 与锁文件一致(${components.length} 个依赖),黑名单扫描通过`);
} else {
  writeFileSync(sbomPath, sbom);
  writeFileSync(licensesPath, markdown);
  scanBlacklist(components);
  console.log(`已生成 SBOM.json 与 THIRD-PARTY-LICENSES.md(${components.length} 个依赖)`);
}
