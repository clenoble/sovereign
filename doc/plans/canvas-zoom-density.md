# Canvas zoom & visual density — follow-up plan

## Status

**v0.0.3 / PII branch (current):**
- `ZOOM_MAX = 20` — users can reach 10-minute tick intervals.
- Cards (`CanvasCard.svelte`) and message circles (`Canvas.svelte`) cap their
  visual size once `zoom > MAX_VISUAL_ZOOM` (= 1.5). Past that, cards apply
  an inverse-scale transform; message radius is computed as
  `MSG_RADIUS * MAX_VISUAL_ZOOM / zoom` so the on-screen size stays constant
  while the time axis keeps compressing.
- Date ticks render in screen space after `ctx.restore()`, sticky to the top
  of the viewport regardless of pan/zoom.
- Wheel: plain = vertical pan, Shift = horizontal pan, Ctrl/Meta/Alt = zoom.

This was scoped as **Option B** in the iteration discussion. It unblocks
hour/minute-level zooming without making cards or message circles take
over the screen.

## Open issue: lane height still scales with zoom

`LANE_HEIGHT` (120 world units) is multiplied by `zoom` everywhere it's
drawn (lane backgrounds, separators, card baseline positions), so at
zoom 20 a single lane occupies 2400px on screen — only 1–2 lanes fit in
a typical viewport, and the cards (capped at ~300px tall) float in
mostly empty vertical space.

Fixing this is harder than the card cap because lane y-positions are the
**anchor coordinates** for cards and message circles. If we shrink lane
height visually we also have to shrink the y-offsets of everything anchored
to a lane.

## Option B+ — extend the cap to lanes

Apply the same `MAX_VISUAL_ZOOM` cap to lanes:

1. Define `verticalScale = min(zoom, MAX_VISUAL_ZOOM)` separate from
   `zoom` (which stays full for the time axis).
2. The card layer's transform becomes `translate(panX, panY) scale(zoom, verticalScale)`
   — non-uniform scale: time axis fully zoomed, vertical content capped.
3. Cards and message circles are positioned at `(spatial_x, spatial_y)` in
   world units. With non-uniform scale, screen position becomes
   `(panX + x*zoom, panY + y*verticalScale)` automatically.
4. Drop the per-card inverse-scale transform — non-uniform parent scale
   handles it.
5. Canvas drawing of lanes: replace `i * LANE_HEIGHT` with
   `i * LANE_HEIGHT` rendered inside `ctx.scale(zoom, verticalScale)`.
   The lane separator at world y=120 lands at screen y=120 * verticalScale,
   which stays bounded.

**Risks / edge cases:**
- Drag-and-drop coordinates (`moveCard`, `snapToLane`): currently use
  `dy / camera.zoom`. Would need to use `dy / verticalScale` to keep
  the drag movement matching the on-screen pointer.
- Hit-testing for message clicks (`checkMessageClick`): currently maps
  `worldY = (screenY - panY) / zoom`. Would become `/ verticalScale`.
- The minimap viewport rectangle: needs the same split treatment.

This is ~1–2 hours of careful work. Worth doing before v0.0.4 ships.

## Option C — user-configurable density

Persist user preferences for visual sizes:

```toml
[ui]
lane_height = 120       # world units; default 120
card_width = 200
card_height = 80
message_radius = 30
date_tick_font_px = 10
max_visual_zoom = 1.5
```

Surface these in the Settings panel under a "Canvas density" section with
either sliders (compact / comfortable / spacious presets) or numeric
inputs. Read on app start; pass into the canvas store via initial state.

**Why later, not now:**
- Settings UI plumbing (form, save command, validation) is independent
  work.
- Hard to tune defaults without first seeing how Option B+ feels.
- Most of the value comes from sensible defaults, not knobs.

Recommended sequence: ship Option B+ → live with the result for a week →
add Option C only if specific users still want it.

## Notes for v0.0.4

- The current sticky-tick implementation includes a 24px tall semi-transparent
  strip at the top of the canvas. If the lane-height cap (Option B+) lands,
  consider whether the strip should also serve as the "current zoom level"
  indicator (e.g. "1 day" / "6 hours" / "10 min" label on the right edge).
- The `home()` function currently picks a zoom that fits all lanes vertically.
  With Option B+, vertical fit becomes trivial — `home()` could instead
  default to a zoom level matching the user's preferred date-tick interval
  (e.g. always start at "daily").
