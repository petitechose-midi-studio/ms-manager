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
