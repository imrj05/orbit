# Vendored gpui 0.2.2 — frosted-glass backdrop pass

This directory is the published `gpui` 0.2.2 crate from crates.io, vendored so
Orbit can add one capability upstream does not have: a **backdrop blur**, the
Primitive that CSS calls `backdrop-filter: blur()`.

Orbit's wallpaper system paints one image layer and gives every surface above it
a translucent fill. That alone shows the photograph *sharp* through a pane. Glass
is the other half — the picture goes soft where a pane sits over it — and it
cannot be faked with a pre-blurred image copy, because a popover must blur what
is actually behind it (a transcript, a diff, another pane), not just the
photograph.

Four places differ from the crates.io source to add the glass pass. Everything
else is untouched, so re-vendoring a 0.2.x release means replaying these files.

Two further changes are lint cleanliness, not features, and must be replayed
too:

- `src/taffy.rs`'s grid-repeat helper spells its float literals `0.0_f32` /
  `1.0_f32`. `length()`/`fr()` take `Into<f32>`, and an untyped literal used to
  fall back from `f64` silently; newer toolchains warn (future-incompatible),
  which fails `-D warnings` builds.
- `Cargo.toml`'s `[lints.clippy]` table allows `derivable_impls`,
  `unnecessary_unwrap` and `approx_constant`. This release trips them, upstream's
  workspace policy silences them, and they are style nits in code Orbit does not
  own — the flagged constant is the frost marker's hue sentinel, not a
  mathematical constant. With these, the vendored crate lints clean on its own
  instead of only through the app.

A third change is unrelated to glass: Linux can keep running with no windows
open, for Orbit's tray icon (#131). `src/platform.rs` adds
`set_keep_alive_without_windows(bool)`, and the X11 and Wayland clients
(`src/platform/linux/{x11,wayland}/client.rs`) skip `signal.stop()` when their
last window closes while it is set. Upstream always stops there, which made
"hide to tray" impossible on Linux; macOS and Windows are untouched.

## 1. `src/scene.rs` — the marker

A quad asks for frost in its border colour, because that field is per-quad,
carries no layout, and is transparent at rest:

```rust
div().bg(tint).rounded(radius).border_color(gpui::backdrop_blur_marker(12.))
```

`backdrop_blur_marker(sigma)` encodes `sigma` (logical points) in the colour's
lightness behind a hue/saturation sentinel with `a == 0`. `is_backdrop_marker`
recognises it, `backdrop_blur_sigma` decodes it once the quad is in device
pixels, and `backdrop_blur_supported()` reports whether the build can honour it
(macOS only — the pass lives in the Metal renderer).

## 2. `src/style.rs` — the marker reaches the quad

`Style::paint` used to derive the fill quad's `border_color` from the
background and force its alpha to zero, so a style's own `border_color` never
reached it. A marked style now hands the marker to that quad instead, and drops
the separate 1px border quads (their colour is the marker, which is invisible
anyway). A surface that wants both a frost and a visible hairline draws the
hairline as an overlay ring — see `frosted_edge` in the app's
`theme/tokens.rs`.

## 3. `src/window.rs` — points to device pixels

`paint_quad` already converts bounds and corner radii by the scale factor; it
now converts the marker's sigma the same way, so a call site passes points and
never threads a `Window` through a style helper.

## 4. `src/platform/mac/{metal_renderer.rs,shaders.metal}` — the pass

`draw_primitives` splits a quad batch at each marked quad: everything before it
is drawn, the backdrop is blurred in place, and drawing resumes on top of the
frost (starting with the marked quad's own tint).

The blur itself copies the region around the glass — 3σ of bleed so the edges
average the backdrop beyond them instead of fading — through Apple's
`MPSImageGaussianBlur` at full resolution with clamped edges, then composites
the result back through `backdrop_composite_fragment`, which keeps the sharp
backdrop outside the rounded bounds. A downsampled or sparse-tap blur would be
cheaper and would ghost any text under the glass.

Scratch textures are reused across glass frames and cleared on a glass-free
frame or the first draw after a two-second gap. Gaussian kernels remain cached
(up to four) until eviction or renderer destruction; an idle window does not
run a timer to release either cache.

## Re-verifying a re-vendor

- `cargo clippy --lib -p gpui --no-deps -- -D warnings` is clean, so the
  vendored crate can be checked on its own rather than only through the app.
- `cargo test -p orbit-pi glass` covers the marker contract and the theme gate.
- Launch with a wallpaper (Settings → Appearance) and confirm the sidebar and
  composer smear the image behind them, with the wallpaper beside them sharp.
- The pass is macOS-only by construction: `backdrop_blur_supported()` is false
  elsewhere, so `Theme::glass_sigma()` returns `None` and panes keep the
  plain translucent fill they had before.
