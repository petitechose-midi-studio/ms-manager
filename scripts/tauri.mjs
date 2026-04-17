import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

import { projectWindowsMsiVersion, shouldInjectWindowsMsiVersion } from "./tauri_versioning.mjs";

const require = createRequire(import.meta.url);
const tauriCliEntrypoint = require.resolve("@tauri-apps/cli/tauri.js");
const packageJsonUrl = new URL("../package.json", import.meta.url);

function readPackageVersion() {
  const packageJson = JSON.parse(readFileSync(packageJsonUrl, "utf8"));
  if (typeof packageJson.version !== "string" || packageJson.version.length === 0) {
    throw new Error("package.json is missing a valid version string.");
  }
  return packageJson.version;
}

const forwardedArgs = process.argv.slice(2);
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

const child = spawnSync(process.execPath, [tauriCliEntrypoint, ...tauriArgs], {
  stdio: "inherit",
  env: process.env,
});

if (child.error) {
  throw child.error;
}

process.exit(child.status ?? 1);
