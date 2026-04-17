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

function resolveOptionalPackage(optionalDependencies, candidates, prefix) {
  for (const candidate of candidates) {
    if (candidate && candidate in optionalDependencies) {
      return candidate;
    }
  }

  if (!prefix) {
    return null;
  }

  return Object.keys(optionalDependencies).find((name) => name.startsWith(prefix)) ?? null;
}

function installOptionalDependency(packageName, version) {
  run("npm", ["install", "--no-save", "--no-audit", "--no-fund", `${packageName}@${version}`]);
}

function ensureOptionalNativeDependency({ packageJsonPath, packageLabel, resolvePackageName }) {
  const packageJson = require(packageJsonPath);
  const optionalDependencies = packageJson.optionalDependencies ?? {};
  const packageName = resolvePackageName(optionalDependencies);
  if (!packageName) {
    throw new Error(`missing ${packageLabel} optional dependency metadata for ${process.platform}/${process.arch}`);
  }

  try {
    require.resolve(`${packageName}/package.json`);
    return;
  } catch {
    const version = optionalDependencies[packageName];
    installOptionalDependency(packageName, version);
  }
}

function ensureRollupNativeBinary() {
  if (process.platform !== "linux") {
    return;
  }

  ensureOptionalNativeDependency({
    packageJsonPath: "rollup/package.json",
    packageLabel: "Rollup",
    resolvePackageName(optionalDependencies) {
      return resolveOptionalPackage(
        optionalDependencies,
        [`@rollup/rollup-${process.platform}-${process.arch}-${detectLinuxLibc()}`],
        `@rollup/rollup-${process.platform}-${process.arch}-`,
      );
    },
  });
}

function ensureTauriCliNativeBinary() {
  ensureOptionalNativeDependency({
    packageJsonPath: "@tauri-apps/cli/package.json",
    packageLabel: "Tauri CLI",
    resolvePackageName(optionalDependencies) {
      if (process.platform === "linux") {
        return resolveOptionalPackage(
          optionalDependencies,
          [`@tauri-apps/cli-linux-${process.arch}-${detectLinuxLibc()}`],
          `@tauri-apps/cli-linux-${process.arch}-`,
        );
      }

      if (process.platform === "darwin") {
        return resolveOptionalPackage(
          optionalDependencies,
          [`@tauri-apps/cli-darwin-${process.arch}`],
          `@tauri-apps/cli-darwin-${process.arch}`,
        );
      }

      if (process.platform === "win32") {
        return resolveOptionalPackage(
          optionalDependencies,
          [`@tauri-apps/cli-win32-${process.arch}-msvc`],
          `@tauri-apps/cli-win32-${process.arch}-`,
        );
      }

      return null;
    },
  });
}

run("npm", ["ci", "--include=optional", "--no-audit", "--no-fund"]);
ensureTauriCliNativeBinary();
ensureRollupNativeBinary();
