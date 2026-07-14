import test from "node:test";
import assert from "node:assert/strict";

import { resolveCargoEnvironment } from "../tauri.mjs";
import { projectWindowsMsiVersion, shouldInjectWindowsMsiVersion } from "../tauri_versioning.mjs";

test("projects stable release versions to an MSI-safe numeric version", () => {
  assert.equal(projectWindowsMsiVersion("0.1.2"), "0.1.299");
});

test("projects beta release versions to a monotonic MSI-safe numeric version", () => {
  assert.equal(projectWindowsMsiVersion("0.1.2-beta.1"), "0.1.200");
  assert.equal(projectWindowsMsiVersion("0.1.2-beta.7"), "0.1.206");
});

test("rejects unsupported prerelease labels", () => {
  assert.throws(
    () => projectWindowsMsiVersion("0.1.2-rc.1"),
    /unsupported application version/,
  );
});

test("rejects beta numbers outside the reserved MSI projection range", () => {
  assert.throws(
    () => projectWindowsMsiVersion("0.1.2-beta.100"),
    /exceeds the supported MSI projection range/,
  );
});

test("injects the MSI override for Windows bundle builds", () => {
  assert.equal(
    shouldInjectWindowsMsiVersion({ platform: "win32", argv: ["build", "--bundles", "msi"] }),
    true,
  );
  assert.equal(
    shouldInjectWindowsMsiVersion({ platform: "win32", argv: ["build"] }),
    true,
  );
});

test("skips the MSI override when no MSI bundle is being built", () => {
  assert.equal(
    shouldInjectWindowsMsiVersion({
      platform: "linux",
      argv: ["build", "--bundles", "deb,rpm"],
    }),
    false,
  );
  assert.equal(
    shouldInjectWindowsMsiVersion({ platform: "linux", argv: ["info"] }),
    false,
  );
});

test("normalizes an existing quoted Cargo directory on the Windows PATH", () => {
  const cargo = String.raw`C:\Rust\bin\cargo.exe`;
  const result = resolveCargoEnvironment({
    env: {
      Path: String.raw`"C:\Rust\bin";C:\Windows\System32`,
      CARGO: String.raw`C:\missing\cargo.exe`,
      USERPROFILE: String.raw`C:\Users\tester`,
    },
    platform: "win32",
    executableExists: (candidate) => candidate === cargo,
  });

  assert.equal(result.cargoExecutable, cargo);
  assert.equal(result.addedToPath, true);
  assert.equal(result.env.Path, String.raw`C:\Rust\bin;C:\Windows\System32`);
  assert.equal(result.env.CARGO, cargo);
});

test("collapses duplicate Windows PATH keys and puts Cargo first", () => {
  const cargo = String.raw`C:\Rust\bin\cargo.exe`;
  const result = resolveCargoEnvironment({
    env: {
      PATH: String.raw`C:\Windows\System32`,
      Path: String.raw`C:\Tools;C:\Rust\bin`,
      USERPROFILE: String.raw`C:\Users\tester`,
    },
    platform: "win32",
    executableExists: (candidate) => candidate === cargo,
  });

  assert.equal(result.cargoExecutable, cargo);
  assert.equal(result.addedToPath, true);
  assert.deepEqual(
    Object.keys(result.env).filter((key) => key.toLowerCase() === "path"),
    ["Path"],
  );
  assert.equal(
    result.env.Path,
    String.raw`C:\Rust\bin;C:\Tools;C:\Windows\System32`,
  );
});

test("adds the standard user Cargo directory when the inherited PATH is stale", () => {
  const cargo = String.raw`C:\Users\tester\.cargo\bin\cargo.exe`;
  const result = resolveCargoEnvironment({
    env: {
      Path: String.raw`C:\Windows\System32`,
      USERPROFILE: String.raw`C:\Users\tester`,
    },
    platform: "win32",
    executableExists: (candidate) => candidate === cargo,
  });

  assert.equal(result.cargoExecutable, cargo);
  assert.equal(result.addedToPath, true);
  assert.equal(
    result.env.Path,
    String.raw`C:\Users\tester\.cargo\bin;C:\Windows\System32`,
  );
});

test("prefers an explicit CARGO_HOME over the standard user directory", () => {
  const cargo = String.raw`D:\toolchains\cargo-home\bin\cargo.exe`;
  const result = resolveCargoEnvironment({
    env: {
      Path: String.raw`C:\Windows\System32`,
      CARGO_HOME: String.raw`D:\toolchains\cargo-home`,
      USERPROFILE: String.raw`C:\Users\tester`,
    },
    platform: "win32",
    executableExists: (candidate) => candidate === cargo,
  });

  assert.equal(result.cargoExecutable, cargo);
  assert.equal(result.env.Path, String.raw`D:\toolchains\cargo-home\bin;C:\Windows\System32`);
});

test("preserves a valid explicit CARGO executable", () => {
  const cargo = String.raw`D:\custom-rust\cargo.exe`;
  const result = resolveCargoEnvironment({
    env: {
      Path: String.raw`C:\Windows\System32`,
      CARGO: cargo,
      USERPROFILE: String.raw`C:\Users\tester`,
    },
    platform: "win32",
    executableExists: (candidate) => candidate === cargo,
  });

  assert.equal(result.cargoExecutable, cargo);
  assert.equal(result.env.CARGO, cargo);
  assert.equal(result.env.Path, String.raw`D:\custom-rust;C:\Windows\System32`);
});

test("leaves the environment unchanged when Cargo is absent", () => {
  const env = { Path: String.raw`C:\Windows\System32` };
  const result = resolveCargoEnvironment({
    env,
    platform: "win32",
    executableExists: () => false,
  });

  assert.deepEqual(result.env, env);
  assert.equal(result.cargoExecutable, null);
  assert.equal(result.addedToPath, false);
});

test("uses POSIX separators and skips an unusable Cargo candidate", () => {
  const cargo = "/home/tester/.cargo/bin/cargo";
  const result = resolveCargoEnvironment({
    env: {
      PATH: "/opt/broken:/usr/bin",
      HOME: "/home/tester",
    },
    platform: "linux",
    executableExists: (candidate) => candidate === cargo,
  });

  assert.equal(result.cargoExecutable, cargo);
  assert.equal(result.addedToPath, true);
  assert.equal(result.env.PATH, "/home/tester/.cargo/bin:/opt/broken:/usr/bin");
});

test("exports an absolute Cargo path when PATH contains a relative directory", () => {
  const cargo = "/workspace/tools/bin/cargo";
  const result = resolveCargoEnvironment({
    env: {
      PATH: "tools/bin:/usr/bin",
      HOME: "/home/tester",
    },
    platform: "linux",
    cwd: "/workspace",
    executableExists: (candidate) => candidate === cargo,
  });

  assert.equal(result.cargoExecutable, cargo);
  assert.equal(result.env.CARGO, cargo);
  assert.equal(result.env.PATH, "/workspace/tools/bin:/usr/bin");
});
