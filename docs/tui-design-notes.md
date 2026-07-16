# p64tool TUI — parked design notes

**Status: PARKED (2026-07-15).** We explored adding a Turbo Vision-style TUI
(`tvision-rs`) but decided to build p64tool's core first (full codeplug decode +
firmware-version gating). These notes capture where the TUI discussion landed so it
can resume later without re-deriving everything.

## Intent

Make P64 configuration *approachable*. The DMR option-space is overwhelming for a
non-expert; the tool should lead with **tasks** ("what do you want to do?") rather
than **data** (100 raw fields). The core real-world use is **setting up a batch of
radios for a group of users** (fleet programming).

## Agreed shape

- The TUI is a new mode inside `p64tool` (e.g. `p64tool tui`, or bare `p64tool`).
  All existing CLI commands stay. The TUI **drives the radio directly** over serial
  (reuses `serial`/`proto`/`codeplug`/`config`/`regs` — adds no protocol logic).
- Base shape = a **full Turbo Vision editor** exposing *all* config properties,
  **plus** special **scenario screens** (wizards) layered on top.
- Main window = **outline/tree (left) + detail pane (right) + menu bar + status
  line**. Single window (the outline is the navigation), modal dialogs for edits.
- **Two lenses on the same `RadioConfig`:**
  - **Advanced view = structural tree** (by aspect: General, Channels, Contacts,
    Talkgroups/RX-groups, Zones, Scan lists, Messages, Alarms, One-touch, Keys),
    every field, terse labels.
  - **Simple view = task-oriented outline** that bundles related records with
    friendly labels + inline help.
- **Interconnections** (channel→contact/talkgroup/key, zone→channels, scan→channels,
  rx-group→contacts) are handled in the detail pane, **not** by tree nesting:
  reference **picker** widgets (dropdown + inline "New…") and a **shuttle** two-list
  transfer widget for memberships; referenced values are jump-links.

## Architecture (mirror `~/checkouts/edaptor`, the author's own `tvision-rs` app)

`edaptor` (an LDAP browser) already solves every pattern; follow its conventions.

- **Facade boundary:** only a new `src/ui/` (or `src/tui/`) may `use tvision_rs`;
  domain layer stays UI-agnostic and headless-testable.
- **One dispatch closure owns the UI:** `Program::run_app(|prog, cmd| dispatch(..))`
  is the only place with `&mut Program` and the only place modal dialogs open. Panes
  never act — they record intent (`requested_*`) and `post` a command; a central
  reconcile handles the "unsaved changes?" guard on navigation.
- **Background serial I/O:** a worker thread owns the serial port; `Request`/
  `Response` enums carry a correlation id; a **50 ms zero-area "pump" view** drains
  the reply channel and broadcasts `REFRESH`. No async runtime; UI never freezes.
  Progress via status line + placeholders (edaptor uses no modal progress dialog).
- **Declarative view-descriptors:** a static Rust table maps each aspect/field to a
  widget kind (bool/enum/int/string/picker/membership) with label + help; the detail
  pane and dialogs are generated from it. (edaptor lowers a `Cfg` enum → validated
  runtime `Kind` enum; value encode/decode lives in the descriptor.)
- **Baseline-diff dirty tracking** (compare field value to load-time baseline).
- Reuse edaptor's **`Shuttle`** widget (`{key,label,locked}` rows) for memberships.
- Target **tvision-rs 0.12** (edaptor is on 0.9/0.10; some APIs moved). For the
  static musl cross-build, likely `default-features = false` (avoid `arboard`).
- Follow edaptor's discipline: TDD, `make check` gate, 4-core cap, borrow discipline
  (never hold a `RefCell` borrow across broadcast/post/exec_view/worker.submit).

## Key simplification vs edaptor

edaptor introspects a **live** schema and re-reads **each entry** as you navigate.
p64tool does **not**: the schema is fixed/compile-time, and the whole codeplug is in
memory (`RadioConfig`) after one read. So navigating is instant, in-memory — the
worker/pump is needed **only** for the two hardware ops (Read-from-radio,
Write-to-radio+verify). References are stored locally (a channel *holds* a contact
index), so we skip edaptor's `fanout_attr` inverse-write and combined multi-record
save. Smaller build: descriptors = a table, worker used in two places.

## State model

Holds: `base: Option<Codeplug>` (raw image edits splice into), `cfg: RadioConfig`
(editable), dirty set, `source` (live radio / dump dir / TOML), `view`, `country`.
**Writing to a radio needs a `base`** — see the version/base notes below; this is a
firmware-safety property, resolved by the core version-gating work, not the TUI.

## Proposed slicing (when unparked)

1. Shell + one aspect end-to-end (General recommended — best exercises the
   Simple/Advanced field curation; least machinery), incl. Read/Write/Open/Save +
   worker/pump.
2. Collections (Channels first, then Contacts, Talkgroups, Zones, Scan, …), reusing
   slice-1 widgets + the Shuttle.
3. Scenario screens: the **group/fleet setup wizard** (define shared config once,
   then a per-radio program loop) + PMR446 presets. Introduces a small **fleet
   artifact** (shared template + per-radio roster) — the one genuinely new data
   concept beyond `config.rs`.

## Dependencies on the core work (why we parked)

The TUI is much stronger once the core lands:
- **Firmware-version gating** makes read/decode/edit/write trustworthy across
  firmwares (see the main version-gating work). The TUI surfaces the radio's
  model/version and enables/blocks write accordingly.
- **Full decode** means nothing is left as opaque bytes; the editor covers 100% of
  the codeplug and offline image generation becomes feasible.

See project memory and the core design docs for the version-gating + full-decode work
that takes priority.
