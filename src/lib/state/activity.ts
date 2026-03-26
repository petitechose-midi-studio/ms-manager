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
    const pad2 = (n: number) => String(n).padStart(2, "0");
    const fmt = (ms: number) => {
      const d = new Date(ms);
      return `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
    };

    return list
      .map((e) => {
        const lvl = e.level.toUpperCase().padEnd(5, " ");
        const sc = e.scope.toUpperCase().padEnd(7, " ");
        return `${fmt(e.ts)} ${lvl} ${sc} ${e.message}`;
      })
      .join("\n");
  }

  return { entries, add, addMany, clear, retain, toText };
}
