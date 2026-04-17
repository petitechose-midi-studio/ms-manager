import { readdirSync, statSync } from "node:fs";
import path from "node:path";

function inferArch(filename) {
  const lower = filename.toLowerCase();
  if (lower.includes("aarch64") || lower.includes("arm64")) {
    return "arm64";
  }
  if (
    lower.includes("x86_64") ||
    lower.includes("x64") ||
    lower.includes("amd64") ||
    lower.includes("intel")
  ) {
    return "x86_64";
  }
  return null;
}

function describeArtifact(filename) {
  const ext = path.extname(filename).toLowerCase();
  if (ext === ".msi") {
    return { id: "package-msi", filename, kind: "package", os: "windows" };
  }
  if (ext === ".deb") {
    return { id: "package-deb", filename, kind: "package", os: "linux" };
  }
  if (ext === ".rpm") {
    return { id: "package-rpm", filename, kind: "package", os: "linux" };
  }
  if (ext === ".dmg") {
    const arch = inferArch(filename);
    if (arch === null) {
      throw new Error(`cannot infer macOS artifact arch from filename: ${filename}`);
    }
    return {
      id: `package-dmg-${arch}`,
      filename,
      kind: "package",
      os: "macos",
      arch,
    };
  }
  throw new Error(`unsupported candidate artifact extension: ${filename}`);
}

function main() {
  const artifactsDir = process.argv[2];
  if (!artifactsDir) {
    throw new Error("usage: node scripts/ci/generate_candidate_artifacts_json.mjs <artifacts-dir>");
  }

  const ignored = new Set(["candidate.json", "candidate.json.sig", "checksums.txt"]);
  const entries = readdirSync(artifactsDir, { withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => entry.name)
    .filter((name) => !ignored.has(name))
    .sort();

  const artifacts = [];
  const ids = new Set();
  for (const name of entries) {
    const fullPath = path.join(artifactsDir, name);
    if (!statSync(fullPath).isFile()) {
      continue;
    }
    const artifact = describeArtifact(name);
    if (ids.has(artifact.id)) {
      throw new Error(`duplicate candidate artifact id: ${artifact.id}`);
    }
    ids.add(artifact.id);
    artifacts.push(artifact);
  }

  if (!artifacts.some((artifact) => artifact.filename.endsWith(".msi"))) {
    throw new Error("candidate artifact set missing .msi package");
  }
  if (!artifacts.some((artifact) => artifact.filename.endsWith(".deb"))) {
    throw new Error("candidate artifact set missing .deb package");
  }
  if (!artifacts.some((artifact) => artifact.filename.endsWith(".rpm"))) {
    throw new Error("candidate artifact set missing .rpm package");
  }
  if (!artifacts.some((artifact) => artifact.filename.endsWith(".dmg"))) {
    throw new Error("candidate artifact set missing .dmg package");
  }

  process.stdout.write(`${JSON.stringify(artifacts, null, 2)}\n`);
}

main();
