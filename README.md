# Tauri + SvelteKit + TypeScript

# MIDI Studio Manager (ms-manager)

End-user app manager for MIDI Studio.

It installs and updates the MIDI Studio bundle by downloading a signed `manifest.json` from the
distribution repo and verifying:

- `manifest.json.sig` (Ed25519)
- each asset sha256

## Release Policy

- `ms-manager` is the control plane for MIDI Studio installation, update, and supervision.
- Its own GitHub release publishes the app packages only.
- The canonical end-user content release remains `petitechose-midi-studio/distribution`.
- `ms-manager` does not define the payload contents and should not embed the canonical release set
  for `loader`, `oc-bridge`, firmware, or the Bitwig extension inside its own release surface.

## Development

Prereqs: https://tauri.app/start/prerequisites/

```bash
npm install
npm run tauri dev
```

## UX Recordings

`ms-manager` archives semantic UX recorder lines emitted by validation firmware builds.
The firmware must explicitly emit `UXR {...}` bridge log lines, typically from the
`midi-studio/core` `dev_ux_recorder` PlatformIO environment.

Recordings are written as bounded NDJSON sessions under the payload root:

```text
<payload-root>/ux-recordings/<instance-id>/<timestamp>_<instance-id>_ux.ndjson
<payload-root>/ux-recordings/index.json
```

The index is schema-versioned and intentionally strict: incompatible index schemas are replaced
with a fresh current index, while malformed current-schema data is reported as an error.

Each session contains:

- a `session_start` line with schema, source, instance, controller serial, and trigger;
- compact `ux_event` lines with action, control, binding, state, and timing fields;
- a `session_end` line when the session is rotated, a new boot marker arrives, or a flash starts.

Business semantics are owned by the firmware recorder source, not by `ms-manager`.
When present, fields such as `mode`, `effect`, `target`, `target_step`, `property`,
`value_label`, and `step_on` are copied into the NDJSON unchanged and used only for
presentation and later `.ux` reconstruction. The manager must not infer these values
from raw control names or view transitions.

Consecutive encoder turns that stay in the same semantic transition are coalesced before they are
written to disk. Encoder records carry `value_kind`: relative controls use signed `delta_milli`,
while normalized controls use absolute `value_milli` and coalesced `first_value_milli` /
`last_value_milli` ranges. Coalesced records also carry `count`, first/last sequence and firmware
timestamps, optional playhead/page range metrics, and duration. Each written event carries `dt_ms`
when a previous UX event exists in the same session. Pending encoder groups are flushed after a
short idle window so the Activity monitor remains live without writing every physical tick.
Singleton encoder turns remain normal events. Session endings expose both `event_count` for written
UX lines and `raw_event_count` for represented physical events. This keeps recordings readable
enough for `.ux` reconstruction while avoiding lossy hard-coded assumptions in the firmware path.

Session creation is opt-in from the firmware logs. If no `UXR` line is observed, no recording is
created. A `UXR {"kind":"session","event":"boot","enabled":1}` marker starts a fresh session and
closes any previous session for that instance. The Activity panel exposes a dedicated `ux` filter,
an `ux folder` action, and a `new ux` action that rotates the current instance into a new NDJSON
without rebooting the controller. UX Activity entries carry structured presentation metadata so the
UI can color controls, values, states, and metrics without embedding display markup in the logs.

### npm note (platform native deps)

If you get errors about missing platform-native packages (for example
`@rollup/rollup-win32-x64-msvc`), check whether you're overriding npm's platform detection:

```bash
npm config get os
```

Normally this should print `null` unless you explicitly set an override. Remove any `os=`/`cpu=`
settings from your `.npmrc` and reinstall dependencies.

## Distribution

See `petitechose-midi-studio/distribution`.
