import { readFileSync } from "node:fs";

import { projectWindowsMsiVersion } from "../tauri_versioning.mjs";

function fail(message) {
  console.error(message);
  process.exit(1);
}

function readJson(path) {
  return JSON.parse(readFileSync(new URL(path, import.meta.url), "utf8"));
}

function readCargoPackageVersion() {
  const cargoToml = readFileSync(new URL("../../src-tauri/Cargo.toml", import.meta.url), "utf8");
  const packageSectionIndex = cargoToml.indexOf("[package]");
  if (packageSectionIndex < 0) {
    fail("src-tauri/Cargo.toml is missing a [package] section.");
  }

  const packageSection = cargoToml.slice(packageSectionIndex);
  const match = packageSection.match(/^version\s*=\s*"([^"]+)"\s*$/m);
  if (!match) {
    fail("src-tauri/Cargo.toml is missing a package version.");
  }
  return match[1];
}

function readCargoLockPackageVersion() {
  const cargoLock = readFileSync(new URL("../../src-tauri/Cargo.lock", import.meta.url), "utf8");
  const match = cargoLock.match(
    /\[\[package\]\]\r?\nname = "ms-manager"\r?\nversion = "([^"]+)"/m,
  );
  if (!match) {
    fail('src-tauri/Cargo.lock is missing the root "ms-manager" package entry.');
  }
  return match[1];
}

const packageJson = readJson("../../package.json");
const tauriConfig = readJson("../../src-tauri/tauri.conf.json");
const cargoVersion = readCargoPackageVersion();
const cargoLockVersion = readCargoLockPackageVersion();

const packageVersion = packageJson.version;
if (typeof packageVersion !== "string" || packageVersion.length === 0) {
  fail("package.json is missing a valid version string.");
}

if (
  tauriConfig.version !== packageVersion ||
  cargoVersion !== packageVersion ||
  cargoLockVersion !== packageVersion
) {
  fail(
    [
      "ms-manager version files are out of sync.",
      `package.json=${packageVersion}`,
      `src-tauri/tauri.conf.json=${tauriConfig.version}`,
      `src-tauri/Cargo.toml=${cargoVersion}`,
      `src-tauri/Cargo.lock=${cargoLockVersion}`,
    ].join("\n"),
  );
}

const msiVersion = projectWindowsMsiVersion(packageVersion);
console.log(`app versions are aligned; Windows MSI version projection: ${packageVersion} -> ${msiVersion}`);
