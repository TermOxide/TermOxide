# Styling system — implementation spec

**Project:** signal-driven, JSX-like TUI framework in Rust
**Status:** design complete, unimplemented
**Companion:** `adr/index.html` — same decisions with reasoning and rejected alternatives. This document
states requirements only. When something here looks arbitrary, the ADR says why.

Read §1 and §2 before writing any code. Everything else can be read on demand.

---

## 1. Invariants

These hold everywhere. A change that violates one is a design change, not a bug fix.

1. **Style is a value, not a language.** No CSS/SCSS parser, no selectors, no specificity, no cascade,
   no string-addressable property namespace in the public API.
2. **A style change never re-runs a component function.** It re-runs one closure and writes into one
   node.
3. **One width authority.** Every width query in the process — layout, paint, truncation, wrapping,
   cursor placement, mouse hit-testing — goes through the injected oracle. A direct call to
   `unicode_width::UnicodeWidthStr::width` anywhere is a defect.
4. **Colours stay symbolic until paint.** No `Style` ever holds a resolved escape sequence.
5. **Children tile their parent exactly.** No gaps, no overflow from rounding.
6. **Clip at paint, never clamp at layout.** Children are laid out at natural size.
7. **Layout and paint read `presented_style`, never `target_style`.**

---

## 2. Core types

```rust
/// Partial by construction. Every field is tri-state.
pub struct Style { /* ~40 fields, all Property<T> */ }

pub enum Property<T> { Unset, Inherit, Value(T) }

pub enum Unit { Cells(i16), Percent(f32), Fr(f32), Auto }

pub enum Color {
    Rgb(u8, u8, u8),
    Base16(u8),      // 0..=15, the user's terminal palette
    Semantic(Sem),   // Accent, Error, Warning, Surface, Muted, …
}

pub enum Layer { Base, Overlay }        // v1. Sort key is (Layer, i32).

pub struct TerminalProfile {
    pub generation: u32,
    pub color_tier: ColorTier,          // TrueColor | Ansi256 | Ansi16 | Mono
    pub palette: Option<NamedPalette>,  // e.g. a Catppuccin flavour
    pub width: Arc<dyn WidthOracle>,
    pub mouse: bool,
    pub osc66: bool,
    pub mode2027: bool,
}
```

### 2.1 Property classification

Every property carries a compile-time class. This drives dirty-bit selection and animatability.

| Class | Meaning | Examples |
|---|---|---|
| `Paint` | Cannot move a rectangle | `fg`, `bg`, `bold`, `underline`, `border_style`, `text_overflow` |
| `Layout` | Can move a rectangle | `width`, `min_width`, `flex_grow`, `display`, `text_wrap` |
| `Both` | Moves the box and changes what fills it | `padding`, `border_sides` |

Store the class in a `const` table keyed by property index. Do not derive it at runtime.

### 2.2 Property table

Paint: `fg`, `bg`, `bold`, `dim`, `italic`, `underline`, `strikethrough`, `reverse`, `border_style`
(`Single | Double | Rounded | Thick | Ascii | None`, **per box, never per side**), `text_overflow`
(`Clip | Ellipsis`).

Layout: `display`, `width`, `height`, `min_width`, `max_width`, `min_height`, `max_height`,
`flex_direction`, `flex_grow`, `flex_shrink`, `flex_basis`, `align_items`, `align_self`,
`justify_content`, `gap`, `position`, `inset_*`, `text_wrap` (`None | Char | Word`).

Both: `padding_*`, `border_sides` (bitflags: which of the four sides are drawn).

Not present, deliberately: `box_sizing` (always border-box), `border_width` (always one cell),
`overflow: visible`, `z_index`, `calc()`, `clamp()`, `px`/`em`/`rem`/`vh`/`ch`.

### 2.3 Merge

```rust
impl Style {
    /// Last write wins per property. `other`'s set properties overwrite `self`'s.
    /// Unset in `other` leaves `self` untouched. No specificity. No !important.
    pub fn merge(self, other: Style) -> Style;
}
```

Precedence is argument order and nothing else. Component authors write merge chains explicitly; the
framework never orchestrates a merge.

### 2.4 Authoring

```rust
const HEADER: Style = style! { bg: Surface, bold: true, pad_x: 2, min_width: 20 };

let dynamic = Style::new().bg(theme.accent()).padding(1);   // runtime, from config
```

`style!` must be `const`-evaluable when all values are literals. The builder must be able to construct
any `Style` the macro can, because applications build styles from their own config files at runtime.

There are **no flattened shorthand props**. `<Box style={s}>` is the only form; `<Box bg=Blue>` does
not exist.

---

## 3. Node and tree

```rust
pub struct Node {
    pub own_style: Style,               // as authored/merged
    pub target_style: Style,            // after inherit resolution
    pub presented_style: Style,         // after animation; layout and paint read THIS
    pub inherited: Rc<Inherited>,       // shared by pointer with parent when not overriding
    pub paint_dirty: bool,
    pub layout_dirty: bool,
    pub subtree_paint_dirty: bool,
    pub layer: (Layer, i32),
    pub style_effect: Option<EffectHandle>,   // node owns it; drop node → drop effect
    // …children, parent, measured cache
}
```

- Nodes live in a **slotmap arena**. Effects hold `NodeId` + a scheduler handle, never `Rc<RefCell<Node>>`.
- **The node owns its style effect.** No effect may outlive the node it writes to.
- `presented_style` exists in v1 even though v1's interpolator is a copy. Do not collapse the two slots.

### 3.1 Inheritance

Paint properties inherit; layout properties do not. `Property::Inherit` opts a non-inheriting property in.

Implement by pointer sharing: a node that overrides no inherited property stores its parent's
`Rc<Inherited>` directly. A parent changing an inherited property allocates one new `Inherited` and sets
its own `subtree_paint_dirty`. **Never walk the subtree to propagate inherited values.**

### 3.2 Invalidation

| Trigger | Sets |
|---|---|
| `Paint` property changed | `paint_dirty` on that node |
| `Layout` or `Both` changed | `layout_dirty` on that node (and ancestors as the solver requires) |
| Inherited property changed | `subtree_paint_dirty` on that node — paint's descent honours it |
| `TerminalProfile.generation` changed | drop all cached measurements, relayout from root |

---

## 4. Pipeline

```
COMPILE TIME   style!{…}                    → const Style
TREE BUILD     component fn                 → nodes in arena
PER CHANGE     ① merge  → target_style
               ② inherit → Rc<Inherited>
PER FRAME      ③ animate → presented_style
               ④ layout  presented[Layout|Both] → taffy → integer edges
               ⑤ paint   presented[Paint|Both] + profile → cells
               ⑥ diff    cell buffer → CUP + SGR bytes
```

There is **no separate style compilation stage** and no stylesheet artifact.

### 4.1 Layout (stage ④)

- **Taffy, low-level API**, over the framework's own node tree. Do not let Taffy own the nodes.
- **Two passes.** Solve base layer → collect portal anchors → solve overlay layers against anchor
  rectangles → paint layers in `(Layer, i32)` order. v1 ships centred modals (no anchor needed); the
  two-pass driver is built regardless.
- **Rounding.** Round absolute edges to integers, derive `size = right_edge − left_edge`. Siblings that
  "should" be equal may differ by one cell. Taffy's rounding is a separable pass (`RoundTree` /
  `round_layout`) — read its source before deciding whether to replace it.
- **Units are literal.** `padding: 1` is one cell each side, visually asymmetric. Provide `pad_x`/`pad_y`.
  The 2:1 convention belongs in widget-library defaults, not the solver.

### 4.2 Leaf measurement

```rust
fn measure(&self, available: Size<AvailableSpace>, oracle: &dyn WidthOracle) -> MeasuredSize;

pub struct MeasuredSize { pub min_content: Size<u16>, pub max_content: Size<u16> }
```

Take `available` from day one even though v1 ignores it — wrapping makes height depend on width, and
retrofitting the signature touches every leaf and the whole driver. Flexbox needs both content sizes to
resolve `flex_grow`.

Cache measurements with the `TerminalProfile.generation` they were taken under. Stale generation →
remeasure.

### 4.3 Paint (stage ⑤)

Colour lowering, in this order:

```
Semantic/Base16 → resolve against palette → Rgb
                → interpolate in Oklab (if animating)
                → quantize to tier (TrueColor | 256 | 16 | Mono | NamedPalette)
                → SGR bytes
```

Quantize **last**. Quantizing before interpolation makes a fade step through arbitrary palette entries.

Emit **absolute cursor positioning (`CUP`)** per run or per line. Never relative movement derived from a
computed width — this is what bounds the damage when a width guess is wrong.

Record per-cell provenance (`table | probed | declared | clamped`) so suspect cells can force full-line
repaints and the grid can be re-derived when capabilities change. This requires retaining source text
per cell, not just resolved glyphs.

---

## 5. Width oracle

Display width is an agreement with the other end of the pty, not a property of a string. Implement a
layered oracle, not a function:

1. **Default:** grapheme-cluster-aware table lookup (`grapheme-width`, `termwiz`, or a libghostty/librio
   wrapper). Segment with `unicode-segmentation`, measure per cluster.
2. **Calibrate:** CPR probing at startup over a small grapheme set. Print probe, `CSI 6 n`, diff the
   reported column. Only method that measures ground truth and the only one that sees through tmux.
   Cache the result.
3. **Upgrade:** DEC mode 2027 via `CSI ? 2027 $ p`. Treat a positive answer as an upgrade signal.
   **Never treat silence or a negative as a downgrade signal** — capable terminals often do not answer.
4. **Dictate:** OSC 66 (`\e]66;w=2;<text>\a`) behind a capability flag, off by default. Architecturally
   the correct fix; availability was Kitty and Foot only as of the research date.
5. **Clamp hostile input.** Zalgo, long virama chains, standalone regional indicators: clamp to 2 cells
   or substitute U+FFFD. Rendering something known to corrupt the grid is worse than a replacement
   character.

The oracle is a trait object injected at every consuming site. v1 freezes the profile after startup
probing; the generation counter exists so mid-session reprobing is a data change later, not a refactor.

---

## 6. Interactive state

The framework maintains per-node flags and exposes them as signals: `hovered`, `focused`, `active`,
`disabled`. There is no pseudo-class engine.

```rust
let mut s = BASE.merge(theme.button()).merge(props.style);
if hovered() { s = s.merge(HOVER); }
if disabled() { s = s.merge(DISABLED); }
```

**Lint:** a component that defines a hover style with no focus equivalent warns. Mouse reporting is
unreliable under SSH and multiplexers, so hover-only affordances disappear in those environments.

---

## 7. Component styling contract

Components expose **named style slots** as ordinary typed props:

```rust
#[component]
fn DataTable(
    style: Option<Style>,
    header_style: Option<Style>,
    row_style: Option<Style>,
    selected_row_style: Option<Style>,
) -> impl View
```

Plus a **theme context** provider for cross-cutting defaults. There is no `::part()` escape hatch and no
selector matching. Slot names are public API and rustdoc is the discovery mechanism.

---

## 8. v1 scope

**In:** the `Style` type, `style!` + builder, merge, inheritance, node-level reactive writes, dirty bits,
Taffy integration with edge rounding, border-box, one-cell borders, clip-at-paint, single overlay layer,
centred modals, two-pass driver, colour degradation with palette quantization, layered width oracle with
startup probing, `text_wrap: Char` plus approximate `Word`, paint-property animation.

**Out of v1, structurally preserved:**

| Capability | What must be true now |
|---|---|
| Scrolling | Clip at paint; children laid out at natural size |
| UAX #14 line breaking | `text_wrap` already in `Style`, `Layout`-classified |
| Anchored dropdowns/tooltips | Two-pass layout driver exists |
| Layout animation | Animator before layout; two style slots per node |
| Mid-session reprobing | Generation counter on every cached measurement |
| Finer overlay ordering | Sort key is `(Layer, i32)` — never CSS z-index semantics |
| RTL / bidi | Cell writer indexed by position, not append order |

---

## 9. Unverified — check before relying on

1. Taffy's `round_layout` algorithm. May already round absolute coordinates, in which case the default
   rounding suffices and §4.1's replacement is unnecessary.
2. Whether current `unicode-width` handles VS15/VS16 presentation selectors. The criticism on record may
   predate recent releases.
3. Whether librio's `cluster_width` is published to crates.io or in-tree only.
4. Ratatui's internals — specifically where and how it computes symbol width. The argument for a custom
   renderer holds on other grounds (oracle injection, byte-stream coupling, per-cell provenance,
   re-measurement), but the ratatui-specific claim was reasoned, not read.
5. Ghostty's OSC 66 status. Parse-only as of 1.3.0, March 2026.
6. Dioxus Subsecond's tip-crate limitation, which would affect a dev loop built on it while editing the
   framework crate itself.

---

## 10. Glossary

**Cell** — one terminal grid position. Roughly 2:1 tall, font-dependent.
**Node** — one element in the tree; owns a rectangle.
**Property** — one styling attribute of a node.
**Slot** — a named style prop exposing a component's internal region.
**Target style** — the resolved style a node should have.
**Presented style** — what is actually on screen this frame, possibly mid-interpolation.
**Oracle** — the injected width authority.
**Tier** — a colour capability level (truecolor, 256, 16, mono).
**Generation** — a profile version counter; invalidates cached measurements.
