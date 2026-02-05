<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { LogicalSize } from "@tauri-apps/api/dpi";
  import { bridgeLogOpen, payloadRootOpen } from "$lib/api/client";

  import type { ActivityEntry, ActivityFilter } from "$lib/state/activity";
  import { createActivityLog } from "$lib/state/activity";
  import { createDashboard } from "$lib/state/dashboard";

  import HeaderBar from "$lib/ui/HeaderBar.svelte";
  import ChannelDropdown from "$lib/ui/ChannelDropdown.svelte";
  import TagDropdown from "$lib/ui/TagDropdown.svelte";
  import ProfilePicker, { type ProfileOption } from "$lib/ui/ProfilePicker.svelte";
  import ContextMenu from "$lib/ui/ContextMenu.svelte";
  import FlashFirmwareModal from "$lib/ui/FlashFirmwareModal.svelte";
  import RelocatePayloadModal from "$lib/ui/RelocatePayloadModal.svelte";
  import ActivityDrawer from "$lib/ui/ActivityDrawer.svelte";
  import ControllerStatus from "$lib/ui/ControllerStatus.svelte";

  const activity = createActivityLog(800);
  const dash = createDashboard(activity);
  const dashState = dash.state;
  const activityEntries = activity.entries;

  let cleanup: (() => void) | null = null;

  onMount(() => {
    void (async () => {
      try {
        const win = getCurrentWindow();
        await win.setMinSize(new LogicalSize(640, 480));
      } catch {
        // ignore
      }
    })();

    void (async () => {
      cleanup = await dash.start();
    })();

    return () => {
      cleanup?.();
    };
  });

  function labelForProfile(id: string): string {
    if (id === "default") return "Standalone";
    if (id === "bitwig") return "Bitwig";
    return id;
  }

  $: profileOptions = $dashState.profileOptions.map((id) => ({
    id,
    label: labelForProfile(id),
    icons:
      id === "default"
        ? ["controller"]
        : id === "bitwig"
          ? ["controller", "bitwig"]
          : undefined,
  })) as ProfileOption[];

  $: hostLabel = $dashState.installed
    ? `${$dashState.installed.channel}:${$dashState.installed.tag}:${$dashState.installed.profile}`
    : "";

  $: versionValue = $dashState.pinnedTag ?? "";
  $: versionOptions = [
    { value: "", label: "latest" },
    ...$dashState.tags.map((t) => ({ value: t, label: t })),
  ];

  function fmtLastFlashed(ms: number): string {
    const age = Math.max(0, Date.now() - ms);
    const m = Math.floor(age / 60000);
    if (m < 1) return "just now";
    if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h ago`;
    const d = Math.floor(h / 24);
    return `${d}d ago`;
  }

  $: flashRecommended = (() => {
    const s = $dashState;
    if (!s.device.connected) return false;
    if (!s.hostInstalled) return false;
    if (!s.installed) return false;
    const desired = {
      channel: s.installed.channel,
      tag: s.installed.tag,
      profile: s.profile,
    };

    if (!s.lastFlashed) return true;
    return (
      desired.channel !== s.lastFlashed.channel ||
      desired.tag !== s.lastFlashed.tag ||
      desired.profile !== s.lastFlashed.profile
    );
  })();

  $: flashRecommendedKind = (() => {
    const s = $dashState;
    if (!s.device.connected) return "info";
    if (!s.hostInstalled) return "info";
    if (!s.installed) return "info";
    if (!s.lastFlashed) return "warn";
    if (s.profile !== s.lastFlashed.profile) return "warn";
    if (s.installed.channel !== s.lastFlashed.channel) return "info";
    if (s.installed.tag !== s.lastFlashed.tag) return "info";
    return "info";
  })();

  async function onProfileSelect(id: string) {
    if (
      $dashState.installing ||
      $dashState.flashing ||
      $dashState.relocating ||
      $dashState.savingSettings
    )
      return;
    if (id === $dashState.profile) return;
    await dash.setProfile(id);
    // User choice: keep host profile even if flash is cancelled.
    dash.openFlashModal(id);
  }

  function onProfileContext(id: string, x: number, y: number) {
    if (
      $dashState.installing ||
      $dashState.flashing ||
      $dashState.relocating ||
      $dashState.savingSettings
    )
      return;
    dash.openContextMenu(id, x, y);
  }

  function onMenuSelect(itemId: string) {
    const target = $dashState.ctxMenu.targetProfile;
    dash.closeContextMenu();
    if (itemId === "flash" && target) {
      dash.openFlashModal(target);
    }
  }

  async function copyActivity(entries: ActivityEntry[], filter: ActivityFilter) {
    const visible = filter === "all" ? entries : entries.filter((e) => e.scope === filter);
    const text = activity.toText(visible);
    try {
      await navigator.clipboard.writeText(text);
    } catch (e) {
      activity.add("warn", "ui", "copy failed", e);
    }
  }

  async function openPayloadRoot() {
    try {
      await payloadRootOpen();
    } catch (e) {
      const err = e as { code?: string; message?: string };
      const msg = typeof err?.message === "string" ? err.message : String(e);
      activity.add("warn", "ui", `open folder failed: ${msg}`, e);
    }
  }

  async function openBridgeLogs() {
    try {
      await bridgeLogOpen();
    } catch (e) {
      const err = e as { code?: string; message?: string };
      const msg = typeof err?.message === "string" ? err.message : String(e);
      activity.add("warn", "ui", `open bridge logs failed: ${msg}`, e);
    }
  }
</script>

<div class="page">
  <HeaderBar
    hostInstalled={$dashState.hostInstalled}
    hostLabel={hostLabel}
    device={$dashState.device}
    platform={$dashState.platform}
  />

  <div class="grid">
    <section class="panel">
      <div class="panelHead">
        <div class="panelTitle">Feed</div>
      </div>

      <div class="panelBody">
        <div class="section">
          <div class="feedRow">
            <ChannelDropdown
              value={$dashState.channel}
              disabled={
                $dashState.installing ||
                $dashState.flashing ||
                $dashState.relocating ||
                $dashState.savingSettings
              }
              onChange={(c) => dash.setChannel(c)}
            />
            <TagDropdown
              value={versionValue}
              options={versionOptions}
              disabled={
                $dashState.loadingTags ||
                $dashState.installing ||
                $dashState.flashing ||
                $dashState.relocating ||
                $dashState.savingSettings
              }
              onChange={(v) => dash.setPinnedTag(v === "" ? null : v)}
            />
          </div>
        </div>

        <div class="section">
          <ProfilePicker
            value={$dashState.profile}
            options={profileOptions}
            disabled={
              $dashState.installing ||
              $dashState.flashing ||
              $dashState.relocating ||
              $dashState.savingSettings
            }
            onSelect={onProfileSelect}
            onContext={onProfileContext}
          />
          <div class="hint">Click: select + suggest flash. Right click: firmware actions.</div>
        </div>

        <div class="section">
          <div class="sectionTitle">Actions</div>
          <div class="actions">
            <button
              class="btn primary"
              type="button"
              disabled={
                $dashState.installing ||
                $dashState.flashing ||
                $dashState.relocating ||
                $dashState.savingSettings
              }
              onclick={() => dash.install()}
            >
              {$dashState.installing ? "Installing…" : "Install / Update"}
            </button>
            <button
              class="btn"
              type="button"
              disabled={
                $dashState.loadingRelease ||
                $dashState.installing ||
                $dashState.flashing ||
                $dashState.relocating ||
                $dashState.savingSettings
              }
              onclick={() => dash.refreshRelease()}
            >
              {$dashState.loadingRelease ? "Checking…" : "Check"}
            </button>
          </div>
          {#if $dashState.now}
            <div class="now">{$dashState.now}</div>
          {/if}
          {#if $dashState.error}
            <div class="err">{$dashState.error.message}</div>
          {/if}
        </div>
      </div>
    </section>

    <section class="panel">
      <div class="panelHead">
        <div class="panelTitle">Status</div>
      </div>

      <div class="panelBody">
        <div class="section">
          <div class="sectionTitle">Host</div>
          <div class="kv">
            <div class="k">Installed</div>
            <div class="v">{$dashState.hostInstalled ? hostLabel : "not installed"}</div>
            <div class="k">Root</div>
            <div class="v rootRow">
              <span class="path">{$dashState.payloadRoot ?? "-"}</span>
              <span class="rootActions">
                <button
                  class="mini"
                  type="button"
                  disabled={!$dashState.payloadRoot || $dashState.relocating}
                  onclick={openPayloadRoot}
                >
                  Open
                </button>
                <button
                  class="mini"
                  type="button"
                  disabled={
                    $dashState.installing ||
                    $dashState.flashing ||
                    $dashState.relocating ||
                    $dashState.savingSettings
                  }
                  onclick={() => dash.openRelocateModal()}
                >
                  Move…
                </button>
              </span>
            </div>
          </div>
        </div>

        <div class="section">
          <div class="sectionTitle">Bridge</div>
          <div class="kv">
            <div class="k">Installed</div>
            <div class="v">{$dashState.bridge.installed ? "yes" : "no"}</div>
            <div class="k">Running</div>
            <div class="v">{$dashState.bridge.running ? "yes" : "no"}</div>
            <div class="k">Paused</div>
            <div class="v">{$dashState.bridge.running ? ($dashState.bridge.paused ? "yes" : "no") : "-"}</div>
            <div class="k">Serial</div>
            <div class="v">
              {$dashState.bridge.running ? ($dashState.bridge.serial_open ? "open" : "waiting") : "-"}
            </div>
            <div class="k">Version</div>
            <div class="v">{$dashState.bridge.version ?? "-"}</div>
            <div class="k">Logs</div>
            <div class="v rootRow">
              <span class="path">bridge.log</span>
              <span class="rootActions">
                <button class="mini" type="button" onclick={openBridgeLogs}>Open</button>
              </span>
            </div>
          </div>
          {#if $dashState.bridge.message && !$dashState.bridge.running}
            <div class="muted">{$dashState.bridge.message}</div>
          {/if}
        </div>

        <div class="section">
          <div class="sectionTitle">Controller</div>
          <div class="kv">
            <div class="k">Controller</div>
            <div class="v">
              <ControllerStatus device={$dashState.device} variant="pill" showCount={true} />
            </div>
            <div class="k">Last flashed</div>
            <div class="v">
              {#if $dashState.lastFlashed}
                {$dashState.lastFlashed.channel}:{ $dashState.lastFlashed.tag }:{
                  $dashState.lastFlashed.profile
                }
                <span class="muted">({fmtLastFlashed($dashState.lastFlashed.flashed_at_ms)})</span>
              {:else}
                <span class="muted">unknown</span>
              {/if}
            </div>
          </div>

        {#if flashRecommended}
          <div class="note" data-kind={flashRecommendedKind}>
            <span>Suggested: flash firmware to match selected profile.</span>
            <button
              class="link"
              data-kind={flashRecommendedKind}
              type="button"
              onclick={() => dash.openFlashModal($dashState.profile)}
            >
              Flash now
            </button>
          </div>
        {/if}
      </div>

      <div class="section">
        <div class="sectionTitle">Release</div>
        {#if !$dashState.release}
          <div class="muted">Not checked.</div>
        {:else if !$dashState.release.available}
          <div class="muted">{$dashState.release.message ?? "No release."}</div>
        {:else}
          <div class="kv">
            <div class="k">Channel</div>
            <div class="v">{$dashState.release.channel}</div>
            <div class="k">Tag</div>
            <div class="v">{$dashState.release.tag}</div>
          </div>
        {/if}
      </div>
      </div>
    </section>
  </div>

  <ActivityDrawer
    open={$dashState.activityOpen}
    filter={$dashState.activityFilter}
    entries={$activityEntries}
    onToggle={() => dash.toggleActivity()}
    onFilter={(f) => dash.setActivityFilter(f)}
    onCopy={() => copyActivity($activityEntries, $dashState.activityFilter)}
    onClear={() => activity.clear()}
  />

  <ContextMenu
    open={$dashState.ctxMenu.open}
    x={$dashState.ctxMenu.x}
    y={$dashState.ctxMenu.y}
    items={[{ id: "flash", label: "Flash firmware" }]}
    onSelect={onMenuSelect}
    onClose={() => dash.closeContextMenu()}
  />

  <FlashFirmwareModal
    open={$dashState.flashModal.open}
    targetProfile={$dashState.flashModal.targetProfile}
    hostInstalled={$dashState.hostInstalled}
    installed={$dashState.installed}
    controllerConnected={$dashState.device.connected}
    flashing={$dashState.flashing}
    progress={$dashState.flashProgress}
    ack={$dashState.flashModal.ack}
    onAck={(v) => dash.setFlashAck(v)}
    onCancel={() => dash.cancelFlashModal()}
    onConfirm={() => dash.confirmFlashModal()}
  />

  <RelocatePayloadModal
    open={$dashState.relocateModal.open}
    currentRoot={$dashState.payloadRoot}
    nextRoot={$dashState.relocateModal.nextRoot}
    relocating={$dashState.relocating}
    ack={$dashState.relocateModal.ack}
    onRoot={(v) => dash.setRelocateRoot(v)}
    onBrowse={() => dash.browseRelocateRoot()}
    onAck={(v) => dash.setRelocateAck(v)}
    onCancel={() => dash.cancelRelocateModal()}
    onConfirm={() => dash.confirmRelocateModal()}
  />
</div>

<style>
  .page {
    height: 100vh;
    padding: 14px;
    display: grid;
    grid-template-rows: auto 1fr auto;
    gap: 12px;
  }

  .grid {
    display: grid;
    grid-template-columns: 340px 1fr;
    gap: 12px;
    min-height: 0;
  }

  .panel {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .panelHead {
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    background: rgba(0, 0, 0, 0.08);
  }

  :global(:root[data-theme="light"]) .panelHead {
    background: rgba(0, 0, 0, 0.03);
  }

  .panelTitle {
    color: var(--value);
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-size: 12px;
    line-height: 14px;
  }

  .panelBody {
    padding: 12px;
    padding-right: 10px;
    overflow: auto;
    min-height: 0;
    display: grid;
    gap: 14px;
    align-content: start;
    scrollbar-gutter: stable;
  }

  .section {
    display: grid;
    gap: 10px;
  }

  .feedRow {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    align-items: flex-end;
  }

  .sectionTitle {
    color: var(--muted);
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 11px;
    line-height: 14px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border);
  }

  .actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .btn {
    appearance: none;
    font: inherit;
    padding: 7px 10px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 12px;
    text-align: left;
  }

  .btn.primary {
    background: var(--value);
    color: var(--bg);
    border-color: var(--value);
  }

  .btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .hint {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
  }

  .now {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
    border: 1px dashed var(--border);
    border-radius: 6px;
    padding: 10px 12px;
  }

  .err {
    color: var(--err);
    font-size: 12px;
    line-height: 16px;
    border: 1px solid var(--err);
    border-radius: 6px;
    padding: 10px 12px;
  }

  .kv {
    display: grid;
    grid-template-columns: 120px 1fr;
    gap: 8px 10px;
    align-items: baseline;
  }

  .rootRow {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }

  .path {
    overflow-wrap: anywhere;
  }

  .rootActions {
    display: inline-flex;
    gap: 8px;
    flex: 0 0 auto;
  }

  .mini {
    appearance: none;
    font: inherit;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    border-radius: 6px;
    padding: 6px 8px;
    cursor: pointer;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 11px;
    line-height: 14px;
  }

  .mini:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .k {
    color: var(--muted);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 11px;
    line-height: 14px;
    font-family: var(--font-mono);
  }

  .k::after {
    content: ":";
    opacity: 0.6;
  }

  .v {
    overflow-wrap: anywhere;
    color: var(--fg);
    opacity: 0.86;
    font-family: var(--font-mono);
    font-weight: 600;
  }

  .muted {
    color: var(--muted);
  }

  .note {
    border: 1px dashed var(--border-strong);
    border-radius: 6px;
    padding: 10px 12px;
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    background: var(--bg);
  }

  .note[data-kind="warn"] {
    border-color: var(--warn);
  }

  .link {
    appearance: none;
    font: inherit;
    color: var(--value);
    background: transparent;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    padding: 6px 8px;
    cursor: pointer;
    font-weight: 900;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 11px;
  }

  .link[data-kind="warn"] {
    color: var(--warn);
    border-color: var(--warn);
  }

  @media (max-width: 860px) {
    .grid {
      grid-template-columns: 1fr;
      grid-template-rows: 1fr 1fr;
    }
  }

  @media (max-width: 520px) {
    .actions {
      grid-template-columns: 1fr;
    }
    .kv {
      grid-template-columns: 1fr;
    }
    .note {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
