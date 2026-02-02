# Tauri + SvelteKit + TypeScript
# MIDI Studio Manager (ms-manager)

End-user app manager for MIDI Studio.

It installs and updates the MIDI Studio bundle by downloading a signed `manifest.json` from the
distribution repo and verifying:
- `manifest.json.sig` (Ed25519)
- each asset sha256

## Development

Prereqs: https://tauri.app/start/prerequisites/

```bash
npm install
npm run tauri dev
```

## Distribution

See `petitechose-midi-studio/distribution`.
