import { get } from "svelte/store";

import { appUpdateCheck, appUpdateOpenLatest, listChannelTags, statusGet } from "$lib/api/client";
import type { Channel } from "$lib/api/types";
import {
  applyStatus,
  clearApiError,
  setApiError,
  type DashboardStatusDeps,
} from "$lib/state/dashboard_shared";

export function createDashboardStatusController({ state, activity }: DashboardStatusDeps) {
  async function refreshStatus() {
    activity.add("info", "ui", "status refresh");
    try {
      applyStatus(state, await statusGet());
    } finally {
      state.update((current) => ({ ...current, booting: false }));
    }
  }

  async function loadTagsForChannel(channel: Channel) {
    state.update((current) => ({ ...current, loadingTags: true }));
    try {
      activity.add("info", "net", `list tags channel=${channel}`);
      const tags = await listChannelTags(channel);
      state.update((current) => ({ ...current, tags }));
      activity.add("ok", "net", `tags count=${tags.length}`);
    } catch (error) {
      activity.add("warn", "net", "list tags failed", error);
    } finally {
      state.update((current) => ({ ...current, loadingTags: false }));
    }
  }

  async function checkAppUpdate() {
    state.update((current) => ({ ...current, checkingAppUpdate: true }));
    try {
      activity.add("info", "net", "check app update");
      const appUpdate = await appUpdateCheck();
      state.update((current) => ({ ...current, appUpdate }));
      if (appUpdate.error) {
        activity.add("warn", "net", `app update check failed: ${appUpdate.error}`);
      } else if (appUpdate.available && appUpdate.update) {
        activity.add("ok", "net", `app update available: ${appUpdate.update.version}`);
      } else {
        activity.add("ok", "net", "app is up to date");
      }
    } catch (error) {
      activity.add("warn", "net", "app update check failed", error);
    } finally {
      state.update((current) => ({ ...current, checkingAppUpdate: false }));
    }
  }

  async function installAppUpdate() {
    const snapshot = get(state);
    if (!snapshot.appUpdate?.available || snapshot.installingAppUpdate) return;
    if (snapshot.installing || snapshot.flashing || snapshot.relocating) return;

    state.update((current) => ({ ...current, installingAppUpdate: true }));
    clearApiError(state);
    activity.add("info", "ui", "opening ms-manager latest release page");
    try {
      await appUpdateOpenLatest();
    } catch (error) {
      setApiError(state, activity, error);
    } finally {
      state.update((current) => ({ ...current, installingAppUpdate: false }));
    }
  }

  return {
    refreshStatus,
    loadTagsForChannel,
    checkAppUpdate,
    installAppUpdate,
  };
}
