const STABLE_VERSION_RE = /^(?<major>0|[1-9]\d*)\.(?<minor>0|[1-9]\d*)\.(?<patch>0|[1-9]\d*)$/;
const BETA_VERSION_RE =
  /^(?<major>0|[1-9]\d*)\.(?<minor>0|[1-9]\d*)\.(?<patch>0|[1-9]\d*)-beta\.(?<beta>0|[1-9]\d*)$/;

const MSI_MAJOR_MINOR_MAX = 255;
const MSI_PATCH_MAX = 65_535;
// MSI upgrades compare only the first three fields, so we reserve the third field
// for both patch progression and beta ordering: patch*100 + [0-98 beta, 99 stable].
const MSI_PATCH_STRIDE = 100;
const MSI_STABLE_SLOT = 99;
const MSI_MAX_PATCH_BASE = Math.floor((MSI_PATCH_MAX - MSI_STABLE_SLOT) / MSI_PATCH_STRIDE);

function fail(message) {
  throw new Error(message);
}

function parseIntField(label, value) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    fail(`invalid ${label}: ${value}`);
  }
  return parsed;
}

function assertWithinRange(label, value, max) {
  if (value > max) {
    fail(`${label}=${value} exceeds the MSI limit (${max}).`);
  }
}

export function parseReleaseVersion(version) {
  const stable = STABLE_VERSION_RE.exec(version);
  if (stable?.groups) {
    const major = parseIntField("major", stable.groups.major);
    const minor = parseIntField("minor", stable.groups.minor);
    const patch = parseIntField("patch", stable.groups.patch);
    return { major, minor, patch, channel: "stable", beta: null };
  }

  const beta = BETA_VERSION_RE.exec(version);
  if (beta?.groups) {
    const major = parseIntField("major", beta.groups.major);
    const minor = parseIntField("minor", beta.groups.minor);
    const patch = parseIntField("patch", beta.groups.patch);
    const betaNumber = parseIntField("beta", beta.groups.beta);
    if (betaNumber < 1) {
      fail(`beta prerelease number must be >= 1, got ${betaNumber}.`);
    }
    return { major, minor, patch, channel: "beta", beta: betaNumber };
  }

  fail(
    [
      `unsupported application version: ${version}`,
      "Expected MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH-beta.N.",
      "This matches the ms release policy and is required to derive a valid MSI ProductVersion.",
    ].join(" "),
  );
}

export function projectWindowsMsiVersion(version) {
  const parsed = parseReleaseVersion(version);
  assertWithinRange("major", parsed.major, MSI_MAJOR_MINOR_MAX);
  assertWithinRange("minor", parsed.minor, MSI_MAJOR_MINOR_MAX);
  assertWithinRange("patch", parsed.patch, MSI_MAX_PATCH_BASE);

  if (parsed.channel === "beta") {
    if (parsed.beta > MSI_STABLE_SLOT) {
      fail(
        `beta prerelease number ${parsed.beta} exceeds the supported MSI projection range (1-${MSI_STABLE_SLOT}).`,
      );
    }
    const projectedPatch = parsed.patch * MSI_PATCH_STRIDE + (parsed.beta - 1);
    return `${parsed.major}.${parsed.minor}.${projectedPatch}`;
  }

  const projectedPatch = parsed.patch * MSI_PATCH_STRIDE + MSI_STABLE_SLOT;
  return `${parsed.major}.${parsed.minor}.${projectedPatch}`;
}

export function shouldInjectWindowsMsiVersion({ platform, argv }) {
  if (argv.length === 0) {
    return false;
  }

  const command = argv[0];
  if (command !== "build" && command !== "bundle") {
    return false;
  }

  if (argv.includes("--no-bundle")) {
    return false;
  }

  let requestedBundles = null;
  for (let i = 1; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg !== "--bundles" && arg !== "-b") {
      continue;
    }

    const next = argv[i + 1];
    if (next == null || next.startsWith("-")) {
      requestedBundles = [];
    } else {
      requestedBundles = next
        .split(",")
        .map((bundle) => bundle.trim().toLowerCase())
        .filter(Boolean);
    }
    break;
  }

  if (requestedBundles !== null) {
    return requestedBundles.includes("msi");
  }

  return platform === "win32";
}
