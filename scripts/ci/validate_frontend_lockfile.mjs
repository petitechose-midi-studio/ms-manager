import { readFileSync } from "node:fs";

const packageJson = JSON.parse(readFileSync(new URL("../../package.json", import.meta.url), "utf8"));
const packageLock = JSON.parse(readFileSync(new URL("../../package-lock.json", import.meta.url), "utf8"));

function fail(message) {
  console.error(message);
  process.exit(1);
}

const lockPackages = packageLock.packages ?? {};
const rootPackage = lockPackages[""];

if (!rootPackage) {
  fail("package-lock.json is missing the root package entry.");
}

if (rootPackage.version !== packageJson.version) {
  fail(
    `package-lock.json root version (${rootPackage.version}) does not match package.json (${packageJson.version}). Regenerate the lockfile from a clean checkout.`,
  );
}

const requiredNativePackages = {
  "Tauri CLI": [
    "node_modules/@tauri-apps/cli-linux-x64-gnu",
    "node_modules/@tauri-apps/cli-darwin-arm64",
    "node_modules/@tauri-apps/cli-darwin-x64",
    "node_modules/@tauri-apps/cli-win32-x64-msvc",
  ],
  Rollup: [
    "node_modules/@rollup/rollup-linux-x64-gnu",
    "node_modules/@rollup/rollup-darwin-arm64",
    "node_modules/@rollup/rollup-darwin-x64",
    "node_modules/@rollup/rollup-win32-x64-msvc",
  ],
  esbuild: [
    "node_modules/@esbuild/linux-x64",
    "node_modules/@esbuild/darwin-arm64",
    "node_modules/@esbuild/darwin-x64",
    "node_modules/@esbuild/win32-x64",
  ],
};

const missingEntries = [];

for (const [label, packages] of Object.entries(requiredNativePackages)) {
  for (const packageName of packages) {
    if (!(packageName in lockPackages)) {
      missingEntries.push(`${label}: ${packageName}`);
    }
  }
}

if (missingEntries.length > 0) {
  fail(
    [
      "package-lock.json is missing required native package entries for the supported candidate platforms:",
      ...missingEntries.map((entry) => `- ${entry}`),
      "",
      "Regenerate package-lock.json from a clean checkout with npm 10+ so the lockfile includes Linux, macOS, and Windows optional dependencies.",
    ].join("\n"),
  );
}

console.log("frontend lockfile is multi-platform and in sync");
