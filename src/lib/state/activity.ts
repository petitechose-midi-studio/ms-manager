import { writable } from "svelte/store";

export type ActivityLevel = "info" | "ok" | "warn" | "error";
export type ActivityScope = "ui" | "net" | "install" | "flash" | "device" | "fs";
export type ActivityFilter = "all" | ActivityScope;

export type ActivityEntry = {
  ts: number;
  level: ActivityLevel;
  scope: ActivityScope;
  message: string;
  details?: unknown;
};

export function createActivityLog(limit = 500) {
  const entries = writable<ActivityEntry[]>([]);

  function add(level: ActivityLevel, scope: ActivityScope, message: string, details?: unknown) {
    const next: ActivityEntry = { ts: Date.now(), level, scope, message, details };
    entries.update((cur) => {
      const out = [...cur, next];
      if (out.length > limit) {
        return out.slice(out.length - limit);
      }
      return out;
    });
  }

  function clear() {
    entries.set([]);
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

  return { entries, add, clear, toText };
}
