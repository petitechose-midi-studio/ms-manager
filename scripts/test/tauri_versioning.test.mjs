import test from "node:test";
import assert from "node:assert/strict";

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
