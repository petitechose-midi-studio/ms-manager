<script lang="ts">
  import { urlOpen } from "$lib/api/client";
  import type { MidiInventoryStatus, MidiPortInfo } from "$lib/api/types";

  const WINDOWS_MIDI_DOCS_URL = "https://microsoft.github.io/MIDI/overview/";

  export let inventory: MidiInventoryStatus | null = null;
  export let loading = false;
  export let controllerSerial: string | null = null;
  export let controllerLabel: string | null = null;

  let open = false;
  let popoverStyle = "";
  let closeHandle: number | null = null;

  function stop(event: Event) {
    event.stopPropagation();
  }

  function linkedPorts(ports: MidiPortInfo[], serial: string | null): MidiPortInfo[] {
    if (!serial) return [];
    return ports.filter((port) => port.match_info.controller_serial === serial);
  }

  function directionLabel(direction: MidiPortInfo["direction"]): string {
    if (direction === "input") return "IN";
    if (direction === "output") return "OUT";
    return "I/O";
  }

  function matchLabel(port: MidiPortInfo): string | null {
    if (port.match_info.confidence === "strong") {
      return controllerLabel?.trim() ? `Linked: ${controllerLabel.trim()}` : "Linked";
    }
    if (port.match_info.confidence === "weak") {
      return controllerLabel?.trim() ? `Possible: ${controllerLabel.trim()}` : "Possible";
    }
    return null;
  }

  async function openDocs(event: MouseEvent) {
    stop(event);
    await urlOpen(WINDOWS_MIDI_DOCS_URL);
  }

  function placePopover(trigger: HTMLElement) {
    const rect = trigger.getBoundingClientRect();
    const width = 280;
    const gap = 8;
    const left = Math.min(
      Math.max(12, rect.left),
      Math.max(12, window.innerWidth - width - 12),
    );
    const top = Math.min(rect.bottom + gap, Math.max(12, window.innerHeight - 180));
    popoverStyle = `left:${left}px;top:${top}px;max-width:${width}px;`;
  }

  function openPopover(trigger: HTMLElement) {
    cancelClose();
    placePopover(trigger);
    open = true;
  }

  function closePopover() {
    open = false;
  }

  function scheduleClose() {
    cancelClose();
    closeHandle = window.setTimeout(() => {
      closeHandle = null;
      closePopover();
    }, 80);
  }

  function cancelClose() {
    if (closeHandle != null) {
      clearTimeout(closeHandle);
      closeHandle = null;
    }
  }

  $: allPorts = inventory?.ports ?? [];
  $: ports = linkedPorts(allPorts, controllerSerial);
  $: allStrong = ports.length > 0 && ports.every((port) => port.match_info.confidence === "strong");
  $: hasWeak = ports.some((port) => port.match_info.confidence === "weak");
  $: unresolved = !!controllerSerial && !!inventory && allPorts.length > 0 && ports.length === 0;
  $: needsGuidance = inventory?.provider === "winmm" && (!allStrong || unresolved);
  $: waiting = loading && !inventory;
  $: summary = waiting ? "Waiting MIDI" : `${ports.length} MIDI Port${ports.length === 1 ? "" : "s"}`;
</script>

<div class="midiWrap" class:warn={hasWeak || unresolved}>
  <button
    class="midiTrigger"
    type="button"
    aria-label="System MIDI ports"
    onmouseenter={(event) => openPopover(event.currentTarget as HTMLElement)}
    onmouseleave={scheduleClose}
    onfocus={(event) => openPopover(event.currentTarget as HTMLElement)}
    onblur={scheduleClose}
    onpointerdown={stop}
    onclick={stop}
  >
    <span class="midiDot" aria-hidden="true"></span>
    <span class="midiLabel">{summary}</span>
  </button>

  <div
    class="midiPopover"
    class:open
    style={popoverStyle}
    role="tooltip"
    onmouseenter={cancelClose}
    onmouseleave={scheduleClose}
  >
    <div class="popoverTitle">System MIDI</div>

    {#if waiting}
      <div class="popoverMuted">MIDI inventory is still loading.</div>
    {:else if !inventory}
      <div class="popoverMuted">Inventory not loaded yet.</div>
    {:else if ports.length}
      <div class="portList">
        {#each ports as port}
          <div class="portRow">
            <span class="portDir" data-direction={port.direction}>{directionLabel(port.direction)}</span>
            <span class="portName">{port.name}</span>
            {#if matchLabel(port)}
              <span class="matchTag" class:weak={port.match_info.confidence === "weak"}>
                {matchLabel(port)}
              </span>
            {/if}
          </div>
        {/each}
      </div>
    {:else if inventory.available}
      <div class="popoverWarn">No confidently linked MIDI port for this controller yet.</div>
    {:else}
      <div class="popoverMuted">No system MIDI port reported by the current provider.</div>
    {/if}

    {#if needsGuidance}
      <div class="popoverFoot">
        <div class="popoverMuted">
          Windows fallback mode cannot reliably map MIDI ports to hardware serials.
        </div>
        <button class="docLink" type="button" onclick={openDocs}>Windows MIDI APIs</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .midiWrap {
    position: relative;
    flex: 0 0 auto;
  }

  .midiTrigger {
    appearance: none;
    border: 0;
    background: color-mix(in srgb, var(--muted) 10%, transparent);
    color: var(--muted);
    border-radius: 999px;
    padding: 2px 7px;
    min-height: 20px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font: inherit;
    font-family: var(--font-sans);
    font-size: 10px;
    line-height: 12px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    cursor: default;
  }

  .midiWrap.warn .midiTrigger {
    color: var(--warn);
    background: color-mix(in srgb, var(--warn) 12%, transparent);
  }

  .midiDot {
    width: 6px;
    height: 6px;
    border-radius: 999px;
    background: currentColor;
    opacity: 0.9;
  }

  .midiPopover {
    position: fixed;
    z-index: 20;
    min-width: 240px;
    padding: 10px 11px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: color-mix(in srgb, var(--panel) 96%, black 4%);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.25);
    display: none;
    gap: 8px;
  }

  .midiPopover.open {
    display: grid;
  }

  .popoverTitle {
    color: var(--fg);
    font-family: var(--font-sans);
    font-size: 11px;
    line-height: 14px;
    font-weight: 800;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .portList {
    display: grid;
    gap: 6px;
  }

  .portRow {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 8px;
    align-items: center;
  }

  .portDir,
  .matchTag {
    border-radius: 999px;
    padding: 2px 6px;
    font-size: 10px;
    line-height: 12px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .portDir {
    color: var(--muted);
    background: color-mix(in srgb, var(--muted) 10%, transparent);
  }

  .portDir[data-direction="input"] {
    color: var(--value);
    background: color-mix(in srgb, var(--value) 12%, transparent);
  }

  .portDir[data-direction="output"] {
    color: var(--ok);
    background: color-mix(in srgb, var(--ok) 14%, transparent);
  }

  .portName {
    color: var(--fg);
    font-size: 12px;
    line-height: 16px;
    overflow-wrap: anywhere;
  }

  .matchTag {
    color: var(--value);
    background: color-mix(in srgb, var(--value) 12%, transparent);
  }

  .matchTag.weak {
    color: var(--warn);
    background: color-mix(in srgb, var(--warn) 12%, transparent);
  }

  .popoverWarn,
  .popoverMuted {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
    overflow-wrap: anywhere;
  }

  .popoverWarn {
    color: var(--warn);
  }

  .popoverFoot {
    display: grid;
    gap: 6px;
    padding-top: 2px;
  }

  .docLink {
    appearance: none;
    border: 0;
    background: transparent;
    color: var(--value);
    font: inherit;
    font-family: var(--font-sans);
    font-size: 11px;
    line-height: 14px;
    font-weight: 700;
    padding: 0;
    cursor: pointer;
    text-align: left;
  }

  .docLink:hover {
    text-decoration: underline;
  }
</style>
