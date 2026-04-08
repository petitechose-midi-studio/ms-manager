import type { ArtifactSource, Channel, FirmwareTarget, LastFlashed } from "$lib/api/types";

export function formatEnvironmentLabel(source: ArtifactSource): string {
  return source === "workspace" ? "Development" : "Distribution";
}

export function formatChannelLabel(channel: Channel): string {
  if (channel === "stable") return "Stable";
  if (channel === "beta") return "Beta";
  if (channel === "nightly") return "Nightly";
  return channel;
}

export function formatTargetLabel(target: FirmwareTarget | string): string {
  if (target === "standalone" || target === "default") return "Standalone";
  if (target === "bitwig") return "Bitwig";
  return target;
}

function formatRelativeAge(ms: number): string {
  const age = Math.max(0, Date.now() - ms);
  const minutes = Math.floor(age / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

export function formatDistributionReleaseLabel(tag: string | null | undefined, channel: Channel): string {
  const cleanTag = tag?.trim();
  if (cleanTag) return `${cleanTag} · ${formatChannelLabel(channel)}`;
  return `Latest · ${formatChannelLabel(channel)}`;
}

export function formatSelectedFirmwareLabel(options: {
  target: FirmwareTarget;
  source: ArtifactSource;
  pinnedTag?: string | null;
  installedTag?: string | null;
  installedChannel?: Channel | null;
  installedReady?: boolean;
}): string {
  const target = formatTargetLabel(options.target);
  if (options.source === "workspace") {
    return `${target} / Development`;
  }

  const channel = options.installedChannel ?? "stable";
  const effectiveTag = options.pinnedTag?.trim()
    ? options.pinnedTag
    : options.installedReady
      ? options.installedTag
      : null;

  return `${target} / ${formatEnvironmentLabel(options.source)} / ${formatDistributionReleaseLabel(
    effectiveTag,
    channel,
  )}`;
}

export function formatLastFlashLabel(lastFlashed: LastFlashed | null | undefined): string {
  if (!lastFlashed) return "Last flash: unknown";

  const target = formatTargetLabel(lastFlashed.profile);
  const age = formatRelativeAge(lastFlashed.flashed_at_ms);

  if (lastFlashed.tag === "workspace") {
    return `Last flash: Development · ${target} (${age})`;
  }

  return `Last flash: Distribution · ${lastFlashed.tag} · ${formatChannelLabel(lastFlashed.channel)} · ${target} (${age})`;
}
