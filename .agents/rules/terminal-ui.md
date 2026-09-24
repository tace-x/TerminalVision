# Terminal UI Rules

Applies to every module in `src/ui/`, `src/layout/` and `src/input/`.

## Tooling

- Use Ratatui for rendering.
- Use Crossterm for terminal interaction.
- Do not bypass these crates with raw escape sequences or manual terminal handling.

## Geometry

- Never hardcode terminal coordinates.
- All UI geometry must be calculated from the current terminal `Rect`.
- Borders and separators must originate from layout rectangles.
- Never manually draw fixed-width ASCII layouts.
- Never allow content to overflow its assigned `Rect`.
- Respect terminal resizing.
- Never assume a fixed terminal size.

## Text and width

- Use Unicode display width rather than `String::len()` for visual width.
- Handle long filenames safely, typically by truncation.
- Preserve readable spacing and alignment.

## Presentation

- Do not use colour as the only indicator of state.
- Provide ASCII-compatible fallbacks where practical.
