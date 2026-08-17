# petitechose.audio identity

`petitechose-mark.svg` is the canonical P5-C geometry. The two lockups repeat that geometry so each SVG remains self-contained when opened in Illustrator.

`midi-studio-manager-app-icon.svg` places the unchanged mark on a neutral tile so the application icon remains legible on both light and dark operating-system surfaces.

## Construction

- Canvas: 24 x 24 units.
- Mark: one open path, 3.2-unit constant stroke, round caps and joins.
- Dot: circle centered at `(12.3, 8.6)`, radius `2.05`.
- Clear space: at least one dot diameter around the mark or lockup.
- Minimum display size: 16 px. Prefer 20 px or larger when antialiasing cannot be controlled.

## Color

- Mark and primary text: `#f3f4f6` on dark surfaces.
- Dot: `#f51b4b`.
- Secondary text: `#a1a5ab`.
- Monochrome: set `--petitechose-dot: currentColor` on the canonical mark and use one foreground color.

The product lockup uses the same mark without alteration. `MIDI STUDIO MANAGER` identifies the product; `petitechose.audio` provides the brand continuity.
