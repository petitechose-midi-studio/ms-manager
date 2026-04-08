<script lang="ts" context="module">
  import type { BridgeInstanceStatus, DeviceTarget, MidiInventoryStatus } from "$lib/api/types";

  export type ControllerTabItem =
    | {
        key: string;
        kind: "instance";
        serial: string;
        label: string;
        instance: BridgeInstanceStatus;
      }
    | {
        key: string;
        kind: "unbound";
        serial: string;
        label: string;
        subtitle: string;
        target: DeviceTarget;
      };
</script>

<script lang="ts">
  import BitwigIcon from "$lib/ui/icons/BitwigIcon.svelte";
  import ControllerIcon from "$lib/ui/icons/ControllerIcon.svelte";
  import MidiPortBadge from "$lib/ui/instance/MidiPortBadge.svelte";

  export let tabs: ControllerTabItem[] = [];
  export let activeKey: string | null = null;
  export let midiInventory: MidiInventoryStatus | null = null;
  export let loadingMidiInventory = false;
  export let onSelect: (key: string) => void;
  export let onReorder: (instanceIds: string[]) => void = () => {};
  export let renamingInstanceId: string | null = null;
  export let nameDraft = "";
  export let busy = false;
  export let onBeginRename: (instanceId: string) => void = () => {};
  export let onNameInput: (value: string) => void = () => {};
  export let onSaveName: () => void = () => {};
  export let onTitleKeydown: (event: KeyboardEvent) => void = () => {};

  const DRAG_THRESHOLD_PX = 6;

  let draggedKey: string | null = null;
  let dropKey: string | null = null;
  let pointerKey: string | null = null;
  let pointerId: number | null = null;
  let pointerStartX = 0;
  let pointerStartY = 0;
  let dragActive = false;
  let suppressClick = false;

  function firmwareIconKind(profile?: string | null, fallbackTarget?: string | null): "bitwig" | "controller" {
    if (profile === "bitwig") return "bitwig";
    if (profile === "default") return "controller";
    return fallbackTarget === "bitwig" ? "bitwig" : "controller";
  }

  function instanceDotKind(instance: {
    enabled: boolean;
    serial_open: boolean;
  }): "ok" | "warn" | "muted" {
    if (!instance.enabled) return "warn";
    if (instance.serial_open) return "ok";
    return "muted";
  }

  function isReorderable(tab: ControllerTabItem): tab is Extract<ControllerTabItem, { kind: "instance" }> {
    return tab.kind === "instance";
  }

  function clearDragState() {
    draggedKey = null;
    dropKey = null;
    pointerKey = null;
    pointerId = null;
    pointerStartX = 0;
    pointerStartY = 0;
    dragActive = false;
  }

  function reorderInstances(sourceKey: string, targetKey: string): string[] {
    const instanceTabs = tabs.filter(isReorderable);
    const sourceIndex = instanceTabs.findIndex((tab) => tab.key === sourceKey);
    const targetIndex = instanceTabs.findIndex((tab) => tab.key === targetKey);
    if (sourceIndex < 0 || targetIndex < 0 || sourceIndex === targetIndex) {
      return instanceTabs.map((tab) => tab.instance.instance_id);
    }

    const reordered = [...instanceTabs];
    const [moved] = reordered.splice(sourceIndex, 1);
    reordered.splice(targetIndex, 0, moved);
    return reordered.map((tab) => tab.instance.instance_id);
  }

  function onTabPointerDown(tab: ControllerTabItem, event: PointerEvent) {
    if (!isReorderable(tab) || event.button !== 0) return;
    const target = event.target;
    if (!(target instanceof Element) || !target.closest("[data-drag-handle='true']")) {
      return;
    }

    pointerKey = tab.key;
    pointerId = event.pointerId;
    pointerStartX = event.clientX;
    pointerStartY = event.clientY;
    draggedKey = null;
    dropKey = null;
    dragActive = false;
  }

  function updateDropTarget(clientX: number, clientY: number) {
    if (!draggedKey) return;

    const element = document.elementFromPoint(clientX, clientY);
    const tabButton = element?.closest<HTMLButtonElement>("[data-tab-key]");
    const targetKey = tabButton?.dataset.tabKey ?? null;
    const reorderable = tabButton?.dataset.reorderable === "true";

    if (!targetKey || !reorderable || targetKey === draggedKey) {
      dropKey = null;
      return;
    }

    dropKey = targetKey;
  }

  function finishPointerDrag(commit: boolean) {
    const sourceKey = draggedKey;
    const targetKey = dropKey;
    const didDrag = dragActive;
    clearDragState();

    if (!didDrag) return;

    suppressClick = true;
    queueMicrotask(() => {
      suppressClick = false;
    });

    if (!commit || !sourceKey || !targetKey || sourceKey === targetKey) return;

    const nextInstanceIds = reorderInstances(sourceKey, targetKey);
    onReorder(nextInstanceIds);
  }

  function onWindowPointerMove(event: PointerEvent) {
    if (pointerId == null || event.pointerId !== pointerId || !pointerKey) return;

    if (!dragActive) {
      const dx = event.clientX - pointerStartX;
      const dy = event.clientY - pointerStartY;
      if (Math.hypot(dx, dy) < DRAG_THRESHOLD_PX) return;

      draggedKey = pointerKey;
      dragActive = true;
    }

    updateDropTarget(event.clientX, event.clientY);
  }

  function onWindowPointerUp(event: PointerEvent) {
    if (pointerId == null || event.pointerId !== pointerId) return;
    finishPointerDrag(true);
  }

  function onWindowPointerCancel(event: PointerEvent) {
    if (pointerId == null || event.pointerId !== pointerId) return;
    finishPointerDrag(false);
  }

  function onTabClick(key: string) {
    if (suppressClick) return;
    onSelect(key);
  }

  function onTabKeydown(key: string, event: KeyboardEvent) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onTabClick(key);
    }
  }

  function isRenamingTab(tab: ControllerTabItem): boolean {
    return tab.kind === "instance" && renamingInstanceId === tab.instance.instance_id;
  }

  function autoFocus(node: HTMLInputElement) {
    queueMicrotask(() => {
      node.focus();
      node.select();
    });

    return {};
  }
</script>

<svelte:window onpointermove={onWindowPointerMove} onpointerup={onWindowPointerUp} onpointercancel={onWindowPointerCancel} />

<div class="tabsScroller">
  <div class="tabs">
  {#if !tabs.length}
    <div class="emptyTabs">No controller detected.</div>
  {:else}
    {#each tabs as tab (tab.key)}
      <div
        class="tab"
        class:active={tab.key === activeKey}
        class:dragging={tab.key === draggedKey}
        class:dragTarget={tab.key === dropKey}
        class:reorderable={tab.kind === "instance"}
        role="button"
        tabindex="0"
        aria-pressed={tab.key === activeKey}
        data-tab-key={tab.key}
        data-reorderable={tab.kind === "instance"}
        onclick={() => onTabClick(tab.key)}
        onkeydown={(event) => onTabKeydown(tab.key, event)}
        onpointerdown={(event) => onTabPointerDown(tab, event)}
      >
        <span class="tabTopRow">
          {#if isRenamingTab(tab)}
            <input
              class="tabTitleInput"
              type="text"
              value={nameDraft}
              placeholder={tab.label}
              disabled={busy}
              use:autoFocus
              oninput={(event) => onNameInput((event.currentTarget as HTMLInputElement).value)}
              onkeydown={onTitleKeydown}
              onblur={onSaveName}
              onclick={(event) => event.stopPropagation()}
              ondblclick={(event) => event.stopPropagation()}
              onpointerdown={(event) => event.stopPropagation()}
            />
          {:else}
            {#if tab.kind === "instance"}
              <button
                class="tabTitleButton"
                type="button"
                title="Double-click to rename"
                onclick={(event) => event.stopPropagation()}
                ondblclick={() => onBeginRename(tab.instance.instance_id)}
              >
                {tab.label}
              </button>
            {:else}
              <span class="tabLabel">{tab.label}</span>
            {/if}
          {/if}
          {#if tab.kind === "instance"}
            <span class="dragHandle" data-drag-handle="true" aria-hidden="true" title="Reorder tab">
              <svg class="dragGrip" viewBox="0 0 10 12" focusable="false">
                <circle cx="2" cy="2" r="1.1"></circle>
                <circle cx="8" cy="2" r="1.1"></circle>
                <circle cx="2" cy="6" r="1.1"></circle>
                <circle cx="8" cy="6" r="1.1"></circle>
                <circle cx="2" cy="10" r="1.1"></circle>
                <circle cx="8" cy="10" r="1.1"></circle>
              </svg>
            </span>
          {/if}
        </span>
        {#if tab.kind === "instance"}
          <span class="tabMetaRow">
            <span class="tabDot" data-kind={instanceDotKind(tab.instance)} aria-hidden="true"></span>
            <span class="tabMetaIcon" aria-hidden="true">
              {#if firmwareIconKind(tab.instance.last_flashed?.profile, tab.instance.target) === "bitwig"}
                <BitwigIcon size={13} />
              {:else}
                <ControllerIcon size={13} />
              {/if}
            </span>
            <span class="tabMeta">Port {tab.instance.host_udp_port}</span>
            <MidiPortBadge
              inventory={midiInventory}
              loading={loadingMidiInventory}
              controllerSerial={tab.instance.configured_serial}
              controllerLabel={tab.label}
            />
          </span>
        {:else}
          <span class="tabMetaRow">
            <span class="tabDot" data-kind="muted" aria-hidden="true"></span>
            <span class="tabMeta">{tab.subtitle}</span>
            <MidiPortBadge
              inventory={midiInventory}
              loading={loadingMidiInventory}
              controllerSerial={tab.serial}
              controllerLabel={tab.label}
            />
          </span>
        {/if}
      </div>
    {/each}
  {/if}
  </div>
</div>

<style>
  .tabsScroller {
    flex: 0 0 auto;
    overflow-x: auto;
    overflow-y: hidden;
    border-bottom: 1px solid var(--border);
    background: rgba(0, 0, 0, 0.06);
  }

  .tabs {
    display: flex;
    align-items: stretch;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    height: var(--tabs-strip-height);
    min-height: var(--tabs-strip-height);
    box-sizing: border-box;
    overflow: visible;
  }

  :global(:root[data-theme="light"]) .tabsScroller {
    background: rgba(0, 0, 0, 0.03);
  }

  .tab {
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    border-radius: var(--radius-card);
    padding: var(--space-2) var(--space-3);
    min-height: 100%;
    min-width: 236px;
    flex: 0 0 236px;
    display: grid;
    gap: 4px;
    text-align: left;
    cursor: default;
    font: inherit;
    touch-action: none;
    outline: none;
  }

  .tab.active {
    border-color: color-mix(in srgb, var(--value) 72%, white);
    color: var(--fg);
    background: color-mix(in srgb, var(--panel-elevated) 72%, transparent);
  }

  .tab.dragging {
    opacity: 0.78;
  }

  .tab.dragTarget {
    border-color: color-mix(in srgb, var(--value) 58%, var(--border));
    box-shadow:
      inset 3px 0 0 color-mix(in srgb, var(--value) 88%, white),
      inset 0 0 0 1px color-mix(in srgb, var(--value) 12%, transparent);
  }

  .tabTopRow {
    display: flex;
    align-items: start;
    gap: var(--space-2);
    min-width: 0;
  }

  .tabLabel {
    font-family: var(--font-sans);
    font-weight: 700;
    font-size: 13px;
    line-height: 16px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1 1 auto;
  }

  .tabTitleButton {
    appearance: none;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    font-family: var(--font-sans);
    font-weight: 700;
    font-size: 13px;
    line-height: 16px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1 1 auto;
    min-width: 0;
    padding: 0;
    margin: 0;
    text-align: left;
    cursor: default;
  }

  .tabTitleInput {
    appearance: none;
    min-width: 0;
    width: 100%;
    border: 0;
    background: transparent;
    color: var(--fg);
    border-radius: 0;
    padding: 0;
    font: inherit;
    font-family: var(--font-sans);
    font-weight: 700;
    font-size: 13px;
    line-height: 16px;
    flex: 1 1 auto;
    min-height: 16px;
    outline: none;
    box-shadow: none;
    caret-color: var(--fg);
  }

  .tabTitleInput:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .dragHandle {
    width: 18px;
    height: 18px;
    flex: 0 0 auto;
    border-radius: 5px;
    display: inline-grid;
    place-items: center;
    color: var(--muted);
    background: transparent;
    cursor: move;
    transition:
      color 140ms ease,
      background-color 140ms ease,
      opacity 140ms ease;
    opacity: 0.72;
  }

  .tab.dragging .dragHandle,
  .tab.dragTarget .dragHandle,
  .tab.reorderable:hover .dragHandle {
    color: var(--fg);
    opacity: 1;
  }

  .dragHandle:hover {
    background: color-mix(in srgb, var(--value) 12%, transparent);
  }

  .dragGrip {
    width: 8px;
    height: 10px;
    display: block;
    fill: currentColor;
  }

  .tabMeta {
    font-family: var(--font-sans);
    font-weight: 500;
    font-size: 11px;
    line-height: 14px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 0 0 auto;
  }

  .tabMetaRow {
    display: grid;
    grid-template-columns: auto auto max-content auto;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }

  .tabDot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--border-strong);
    flex: 0 0 auto;
  }

  .tabDot[data-kind="ok"] {
    background: var(--ok);
  }

  .tabDot[data-kind="warn"] {
    background: var(--warn);
  }

  .tabMetaIcon {
    width: 14px;
    height: 14px;
    display: inline-grid;
    place-items: center;
    color: var(--muted);
    flex: 0 0 auto;
  }

  .emptyTabs {
    color: var(--muted);
    font-size: 12px;
  }
</style>
