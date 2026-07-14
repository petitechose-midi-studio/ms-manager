import { spawnSync } from "node:child_process";
import { accessSync, constants, readFileSync, statSync } from "node:fs";
import { createRequire } from "node:module";
import { posix, win32 } from "node:path";
import { pathToFileURL } from "node:url";

import { projectWindowsMsiVersion, shouldInjectWindowsMsiVersion } from "./tauri_versioning.mjs";

const packageJsonUrl = new URL("../package.json", import.meta.url);

function readPackageVersion() {
  const packageJson = JSON.parse(readFileSync(packageJsonUrl, "utf8"));
  if (typeof packageJson.version !== "string" || packageJson.version.length === 0) {
    throw new Error("package.json is missing a valid version string.");
  }
  return packageJson.version;
}

function unquotePathEntry(entry) {
  const trimmed = entry.trim();
  if (trimmed.length >= 2 && trimmed.startsWith('"') && trimmed.endsWith('"')) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

function pathStateForEnvironment(env, platform, separator) {
  if (platform !== "win32") {
    return { pathKey: "PATH", pathKeys: ["PATH"], currentPath: env.PATH ?? "" };
  }

  const pathKeys = Object.keys(env).filter((key) => key.toLowerCase() === "path");
  const pathKey = pathKeys.find((key) => key === "Path") ?? pathKeys[0] ?? "Path";
  const orderedKeys = [pathKey, ...pathKeys.filter((key) => key !== pathKey)];
  const currentPath = orderedKeys
    .map((key) => env[key])
    .filter((value) => typeof value === "string" && value.length > 0)
    .join(separator);
  return { pathKey, pathKeys, currentPath };
}

function directoryIdentity(directory, platform, pathApi, cwd) {
  let normalized = pathApi.resolve(cwd, unquotePathEntry(directory));
  const root = pathApi.parse(normalized).root;
  while (normalized.length > root.length && /[\\/]$/.test(normalized)) {
    normalized = normalized.slice(0, -1);
  }
  return platform === "win32" ? normalized.toLowerCase() : normalized;
}

function environmentWithCargoFirst({
  env,
  platform,
  separator,
  pathApi,
  pathKey,
  pathKeys,
  currentPath,
  cargoDirectory,
  cwd,
}) {
  const cargoIdentity = directoryIdentity(cargoDirectory, platform, pathApi, cwd);
  const currentEntries = currentPath.length > 0 ? currentPath.split(separator) : [];
  const remainingEntries = currentEntries
    .filter((entry) => directoryIdentity(entry, platform, pathApi, cwd) !== cargoIdentity);
  const nextPath = [cargoDirectory, ...remainingEntries].join(separator);
  const childEnv = { ...env };

  if (platform === "win32") {
    for (const key of pathKeys) delete childEnv[key];
  }
  childEnv[pathKey] = nextPath;

  return {
    env: childEnv,
    pathAdjusted: nextPath !== currentPath || pathKeys.length > 1,
  };
}

function isUsableExecutable(executable, platform) {
  try {
    if (!statSync(executable).isFile()) return false;
    if (platform !== "win32") accessSync(executable, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

/**
 * Return a child-process environment in which Cargo is resolvable.
 *
 * Desktop shells can retain a PATH captured before rustup was installed. The
 * wrapper validates an explicit CARGO value, PATH, then the standard Cargo
 * homes, without mutating the parent process or requiring a global PATH change.
 */
export function resolveCargoEnvironment({
  env = process.env,
  platform = process.platform,
  cwd = process.cwd(),
  executableExists = (executable) => isUsableExecutable(executable, platform),
} = {}) {
  const pathApi = platform === "win32" ? win32 : posix;
  const separator = platform === "win32" ? ";" : ":";
  const executableName = platform === "win32" ? "cargo.exe" : "cargo";
  const { pathKey, pathKeys, currentPath } = pathStateForEnvironment(
    env,
    platform,
    separator,
  );
  const pathEntries = currentPath
    .split(separator)
    .map(unquotePathEntry)
    .filter((entry) => entry.length > 0);

  const resultForCargo = (cargoDirectory, cargoExecutable) => {
    const child = environmentWithCargoFirst({
      env,
      platform,
      separator,
      pathApi,
      pathKey,
      pathKeys,
      currentPath,
      cargoDirectory,
      cwd,
    });
    return {
      env: { ...child.env, CARGO: cargoExecutable },
      cargoExecutable,
      addedToPath: child.pathAdjusted,
    };
  };

  if (env.CARGO) {
    const configuredCargo = pathApi.resolve(cwd, unquotePathEntry(env.CARGO));
    if (executableExists(configuredCargo)) {
      return resultForCargo(pathApi.dirname(configuredCargo), configuredCargo);
    }
  }

  for (const directory of pathEntries) {
    const absoluteDirectory = pathApi.resolve(cwd, directory);
    const executable = pathApi.join(absoluteDirectory, executableName);
    if (executableExists(executable)) {
      return resultForCargo(absoluteDirectory, executable);
    }
  }

  const candidateDirectories = [];
  if (env.CARGO_HOME) {
    candidateDirectories.push(
      pathApi.resolve(cwd, unquotePathEntry(env.CARGO_HOME), "bin"),
    );
  }
  if (env.USERPROFILE) {
    candidateDirectories.push(
      pathApi.resolve(cwd, unquotePathEntry(env.USERPROFILE), ".cargo", "bin"),
    );
  }
  if (env.HOME) {
    candidateDirectories.push(
      pathApi.resolve(cwd, unquotePathEntry(env.HOME), ".cargo", "bin"),
    );
  }

  const seen = new Set();
  for (const directory of candidateDirectories) {
    const identity = platform === "win32" ? directory.toLowerCase() : directory;
    if (seen.has(identity)) continue;
    seen.add(identity);

    const executable = pathApi.join(directory, executableName);
    if (!executableExists(executable)) continue;

    return resultForCargo(directory, executable);
  }

  return {
    env: { ...env },
    cargoExecutable: null,
    addedToPath: false,
  };
}

function runTauri(forwardedArgs) {
  const require = createRequire(import.meta.url);
  const tauriCliEntrypoint = require.resolve("@tauri-apps/cli/tauri.js");
  const tauriArgs = [...forwardedArgs];

  if (shouldInjectWindowsMsiVersion({ platform: process.platform, argv: forwardedArgs })) {
    const appVersion = readPackageVersion();
    const msiVersion = projectWindowsMsiVersion(appVersion);
    tauriArgs.push(
      "--config",
      JSON.stringify({
        bundle: {
          windows: {
            wix: {
              version: msiVersion,
            },
          },
        },
      }),
    );
    console.error(`[tauri-wrapper] using MSI version ${msiVersion} for app version ${appVersion}`);
  }

  const cargo = resolveCargoEnvironment();
  if (cargo.cargoExecutable) {
    console.error(`[tauri-wrapper] using Cargo executable: ${cargo.cargoExecutable}`);
  }

  const child = spawnSync(process.execPath, [tauriCliEntrypoint, ...tauriArgs], {
    stdio: "inherit",
    env: cargo.env,
  });

  if (child.error) throw child.error;
  return child.status ?? 1;
}

const directEntrypoint = process.argv[1]
  ? pathToFileURL(process.argv[1]).href === import.meta.url
  : false;
if (directEntrypoint) {
  try {
    process.exit(runTauri(process.argv.slice(2)));
  } catch (error) {
    console.error(`[tauri-wrapper] ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  }
}
