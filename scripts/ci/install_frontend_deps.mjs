import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);

function run(command, args) {
  execFileSync(command, args, {
    stdio: "inherit",
    shell: process.platform === "win32",
  });
}

function detectLinuxLibc() {
  const report = process.report?.getReport?.();
  return report?.header?.glibcVersionRuntime ? "gnu" : "musl";
}

function pickRollupLinuxPackage(optionalDependencies) {
  if (process.platform !== "linux") {
    return null;
  }

  const exactName = `@rollup/rollup-${process.platform}-${process.arch}-${detectLinuxLibc()}`;
  if (exactName in optionalDependencies) {
    return exactName;
  }

  const prefix = `@rollup/rollup-${process.platform}-${process.arch}-`;
  return Object.keys(optionalDependencies).find((name) => name.startsWith(prefix)) ?? null;
}

function ensureRollupNativeBinary() {
  if (process.platform !== "linux") {
    return;
  }

  const rollupPackage = require("rollup/package.json");
  const optionalDependencies = rollupPackage.optionalDependencies ?? {};
  const packageName = pickRollupLinuxPackage(optionalDependencies);
  if (!packageName) {
    throw new Error("missing Linux Rollup optional dependency metadata");
  }

  try {
    require.resolve(`${packageName}/package.json`);
    return;
  } catch {
    const version = optionalDependencies[packageName];
    run("npm", ["install", "--no-save", "--no-audit", "--no-fund", `${packageName}@${version}`]);
  }
}

run("npm", ["ci", "--include=optional", "--no-audit", "--no-fund"]);
ensureRollupNativeBinary();
