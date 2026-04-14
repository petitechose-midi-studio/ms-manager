import type { BridgeLogEvent } from "$lib/api/types";
import { writable } from "svelte/store";

export type ActivityLevel = "info" | "ok" | "warn" | "error";
export type ActivityScope = "ui" | "net" | "install" | "flash" | "device" | "fs" | "bridge";
export type ActivityFilter = "all" | "manager" | "flash" | "bridge";

export type ActivityEntry = {
  id: number;
  ts: number;
  level: ActivityLevel;
  scope: ActivityScope;
  message: string;
  details?: unknown;
};

type ActivityMarkerTone = "muted" | "info" | "ok" | "warn" | "error" | "rx" | "tx" | "system";

export type ActivityPresentation = {
  timestamp: string;
  marker: string;
  markerTone: ActivityMarkerTone;
  scopeLabel: string;
  sourceLabel: string | null;
  message: string;
  text: string;
};

const SCOPE_LABELS: Record<ActivityScope, string> = {
  ui: "MANAGER",
  net: "NET",
  install: "INSTALL",
  flash: "FLASH",
  device: "DEVICE",
  fs: "FS",
  bridge: "BRIDGE",
};

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

function pad3(n: number): string {
  return String(n).padStart(3, "0");
}

function formatActivityTimestamp(ms: number): string {
  const d = new Date(ms);
  return `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}.${pad3(d.getMilliseconds())}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isBridgeLogEvent(details: unknown): details is BridgeLogEvent {
  return (
    isRecord(details) &&
    typeof details.message === "string" &&
    typeof details.kind === "string" &&
    typeof details.timestamp === "string" &&
    typeof details.port === "number"
  );
}

function splitSourceLabel(message: string): { sourceLabel: string | null; message: string } {
  const match = /^\[([^\]]+)\]\s+([\s\S]*)$/.exec(message);
  if (!match) {
    return { sourceLabel: null, message };
  }
  return {
    sourceLabel: match[1].trim() || null,
    message: match[2],
  };
}

function activityMarker(entry: ActivityEntry): { marker: string; markerTone: ActivityMarkerTone } {
  if (isBridgeLogEvent(entry.details)) {
    if (entry.details.kind === "protocol_in") {
      return { marker: "< RX", markerTone: "rx" };
    }
    if (entry.details.kind === "protocol_out") {
      return { marker: "> TX", markerTone: "tx" };
    }
    if (entry.details.kind === "system") {
      return { marker: "[SYS]", markerTone: "system" };
    }

    switch (entry.details.level) {
      case "error":
        return { marker: "[ERR]", markerTone: "error" };
      case "warn":
        return { marker: "[WRN]", markerTone: "warn" };
      case "info":
        return { marker: "[INF]", markerTone: "info" };
      case "debug":
        return { marker: "[DBG]", markerTone: "muted" };
      default:
        return { marker: "[LOG]", markerTone: "muted" };
    }
  }

  switch (entry.level) {
    case "error":
      return { marker: "[ERR]", markerTone: "error" };
    case "warn":
      return { marker: "[WRN]", markerTone: "warn" };
    case "ok":
      return { marker: "[OK ]", markerTone: "ok" };
    case "info":
    default:
      return { marker: "[INF]", markerTone: "info" };
  }
}

export function presentActivityEntry(entry: ActivityEntry): ActivityPresentation {
  const bridgeDetails = isBridgeLogEvent(entry.details);
  const sourceParts = bridgeDetails || entry.scope === "bridge"
    ? splitSourceLabel(entry.message)
    : { sourceLabel: null, message: entry.message };
  const { sourceLabel, message } = sourceParts;
  const { marker, markerTone } = activityMarker(entry);
  const timestamp = formatActivityTimestamp(entry.ts);
  const scopeLabel = SCOPE_LABELS[entry.scope];
  const sourceText = sourceLabel ? `[${sourceLabel}] ` : "";
  const text = `${timestamp} ${marker.padEnd(5, " ")} ${scopeLabel.padEnd(7, " ")} ${sourceText}${message}`;

  return {
    timestamp,
    marker,
    markerTone,
    scopeLabel,
    sourceLabel,
    message,
    text,
  };
}

export function formatActivityEntriesText(list: ActivityEntry[]): string {
  return list.map((entry) => presentActivityEntry(entry).text).join("\n");
}

export function matchesActivityFilter(entry: ActivityEntry, filter: ActivityFilter): boolean {
  if (filter === "all") return true;
  if (filter === "flash") return entry.scope === "flash";
  if (filter === "bridge") return entry.scope === "bridge";
  return entry.scope !== "flash" && entry.scope !== "bridge";
}

export function createActivityLog(limit = 500) {
  const entries = writable<ActivityEntry[]>([]);
  let nextId = 1;

  function trim(list: ActivityEntry[]) {
    if (list.length > limit) {
      return list.slice(list.length - limit);
    }
    return list;
  }

  function add(level: ActivityLevel, scope: ActivityScope, message: string, details?: unknown) {
    const next: ActivityEntry = { id: nextId++, ts: Date.now(), level, scope, message, details };
    entries.update((cur) => {
      return trim([...cur, next]);
    });
  }

  function addMany(
    nextEntries: {
      level: ActivityLevel;
      scope: ActivityScope;
      message: string;
      details?: unknown;
    }[],
  ) {
    if (!nextEntries.length) return;
    entries.update((cur) =>
      trim([
        ...cur,
        ...nextEntries.map((entry) => ({
          id: nextId++,
          ts: Date.now(),
          level: entry.level,
          scope: entry.scope,
          message: entry.message,
          details: entry.details,
        })),
      ]),
    );
  }

  function clear() {
    entries.set([]);
  }

  function retain(predicate: (entry: ActivityEntry) => boolean) {
    entries.update((cur) => cur.filter(predicate));
  }

  function toText(list: ActivityEntry[]): string {
    return formatActivityEntriesText(list);
  }

  return { entries, add, addMany, clear, retain, toText };
}
