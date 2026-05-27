# Loom architecture

How the code is shaped and why. Read [README.md](README.md) first for what Loom is supposed to *do*; this doc covers *how it's built*.

## Module map

```
src/
├── Loom.bb                          entry point — boots Blitz3D, loads
│                                    project data, runs the main loop
└── Modules/
    ├── (existing GUE data modules — Items.bb, Actors.bb, etc.)
    │                                read-only consumed; not modified
    └── Loom/
        ├── Theme.bb                 color palette + 2D drawing primitives
        ├── Threads.bb               focus state + back stack + chip primitive
        ├── Browser.bb               category bar + filter input + card grid
        ├── Composer.bb              right-side property panel per kind
        └── Palette.bb               Ctrl+K find-anywhere modal
```

### One-line module summaries

- **`Loom.bb`** — Bootstrap globals + data-loader sequence (mirrors `GUE.bb`'s order) + defines `Type Loom` and constructs an instance. The main loop is `While Loom::renderFrame(app) Wend` — `renderFrame` returns False when Esc exits from an empty state.
- **`Theme.bb`** — Color tokens as `LOOM_STONE_900_R/G/B`-style constants (the design's full palette). Drawing primitives wrap Blitz's `Color/Rect/Line/Text` so callers paint through `LoomFill / LoomGradientV / LoomHRule / LoomText / LoomTextCentered`. Stateless helpers; kept as free functions per the rule of thumb in the BlitzForge skill (no state → free functions are fine).
- **`Threads.bb`** — The centerpiece module. `Type Threads` owns `focusKind$`, `focusID%`, and a `backStack.BBList` of `LoomFocusEntry`. Methods: `create.Threads`, `focus(kind, refID)` (direct set), `jump(kind, refID)` (set + push back stack), `back%()` (pop), `clearStack()`, `lookupName$(kind, refID)`, `renderChip%(...)` (the clickable rounded-rect every reference field uses).
- **`Browser.bb`** — The boot surface. `Type Browser` holds a reference to the shared `Threads` instance + the current `category$`. Six categories (`Actors / Items / Spells / Zones / Factions / Animation Sets`); each has its own per-kind grid method (`drawActorGrid`, `drawItemGrid`, etc.) dispatched from `drawCardGrid`. Click a card → `Threads::focus(self\threads, kind, refID)`.
- **`Composer.bb`** — Right-side panel that appears when something's focused. `Type Composer` holds a reference to the same `Threads` instance. Per-kind body renderer methods (`renderActor`, `renderItem`, …) lay out rows of `label : value` and rows of `label : [thread chip]`. Reads from the data modules' globals (`ActorList`, `ItemList`, `FactionNames$`, `Each AnimSet`, `Each Area`). `width%()` returns 0 when nothing is focused so the Browser knows whether to reserve right-edge space.

### Why Types with Methods (not prefixed free functions)

Loom's UI modules each own state — `Browser` owns the current category, `Composer` owns layout latches, `Threads` owns focus + back stack. The project's canonical OO convention (`Project Manager.bb`, `Framework/RCCEApp.bb`, `Framework/Project/Project.bb`) is **`Type` with `Method`s called via `TypeName::method(self, args)`** for stateful modules; Loom follows that pattern. See [`.claude/skills/blitzforge-language/SKILL.md`](../../.claude/skills/blitzforge-language/SKILL.md) "Module architecture" section for the rule + canonical examples.

The top-level `Type Loom` holds the three sub-instances:

```basic
Type Loom
    Field threads.Threads
    Field browser.Browser
    Field composer.Composer
    Field projectName$
    Field windowWidth%, windowHeight%

    Method create.Loom(w%, h%, name$)
        self\threads = New Threads()
        self\browser = New Browser(self\threads)     // shares Threads
        self\composer = New Composer(self\threads)   // shares Threads
        ...
    End Method

    Method renderFrame%()
        Browser::renderAndUpdate(self\browser, self\windowWidth, self\windowHeight, self\projectName)
        Composer::renderAndUpdate(self\composer, self\windowWidth, self\windowHeight)
        ...
    End Method
End Type
```

The Browser and Composer both receive the same Threads reference at construction, so card clicks (Browser → `Threads::focus`) and chip clicks (Composer → `Threads::jump` via `Threads::renderChip`) write to the same back stack without globals.

## Data flow

```
                       ┌──────────────────────────┐
                       │  Data .dat files on disk │
                       │  (under <project>/Data/) │
                       └─────────────┬────────────┘
                                     │  LoadActors, LoadItems,
                                     │  LoadSpells, ServerLoadArea,
                                     │  LoadFactions, LoadAnimSets, ...
                                     ▼
              ┌──────────────────────────────────────────┐
              │  In-memory type instances + arrays:      │
              │  ActorList(N), ItemList(N), SpellsList,  │
              │  FactionNames$(99), Each Area / AnimSet  │
              └─────────────┬────────────────────┬───────┘
                            │                    │
                read by:    │                    │  written by GUE
                            │                    │  (Loom is read-only)
                            ▼                    │
                  ┌────────────────────┐         │
                  │  Loom UI modules   │         │
                  │  (Browser/Composer)│         │
                  └─────────┬──────────┘         │
                            │                    │
                            ▼                    ▼
                       paint to screen      Save*.dat
                       via Theme.bb
                       primitives
```

**Key invariant:** Loom reads through the exact same `LoadX` functions GUE uses, so the two editors cannot drift in how they parse the file format. The cost is dragging in GUE's data modules wholesale; the benefit is correctness by construction.

## Shared state (the vocabulary)

Lives as fields on the `Threads` instance, which is the source of truth shared between Browser and Composer (both hold a reference set at construction time — no globals):

| Field | Type | Meaning |
|---|---|---|
| `threads\focusKind$` | string | `"" \| "actor" \| "item" \| "spell" \| "zone" \| "faction" \| "animset"` |
| `threads\focusID%` | int | interpretation depends on `focusKind` — see below |
| `threads\backStack.BBList` | `BBList` of `LoomFocusEntry` | navigation trail; popped by Esc |

**`refID` payload per kind** — every Loom module uses these conventions; never deviate:

| Kind | `refID` payload |
|---|---|
| `actor` | `Actor\ID` (array index into `ActorList`) |
| `item` | `Item\ID` (array index into `ItemList`) |
| `spell` | `Spell\ID` (array index into `SpellsList`) |
| `zone` | `Handle(Area)` (round-trips via `Object.Area(handle)`) |
| `faction` | `FactionNames$` array index 0..99 |
| `animset` | `AnimSet\ID` |

## The render loop

`Loom.bb` main loop, simplified — `Loom::renderFrame` returns False when Loom should exit:

```basic
Local app.Loom = New Loom(boot_width, boot_height, projectName)
While Loom::renderFrame(app) = True
Wend
```

Inside `renderFrame`:

```basic
Method renderFrame%()
    Cls
    Browser::renderAndUpdate(self\browser, self\windowWidth, self\windowHeight, self\projectName)
    Composer::renderAndUpdate(self\composer, self\windowWidth, self\windowHeight)  // no-op when focus = ""

    If KeyHit(1)   // Esc
        If Threads::back(self\threads) = False
            If self\threads\focusKind <> ""
                Threads::focus(self\threads, "", 0)        // close composer
                Threads::clearStack(self\threads)
            Else
                Return False                                // exit Loom
            EndIf
        EndIf
    EndIf

    Flip
    Return True
End Method
```

The two render calls are **idempotent and stateless from the caller's POV** — every frame re-reads the data, re-runs hit-tests, re-paints. There's no "switch to a different mode" dispatch; the composer's visibility is purely a function of `self\threads\focusKind`.

`Composer::width(composer)` returns 0 when nothing is focused (else `CMP_W`); the Browser reads this so a future PR can shrink the grid by that many pixels on the right when the composer is visible. Today the composer just overlays.

## Why custom-draw + not F-UI

See [decisions/001-custom-draw-not-fui.md](decisions/001-custom-draw-not-fui.md) for the full rationale. Short version: F-UI is built for utilitarian Windows-style gadgets and cannot render the gradient-heavy, ornament-heavy aesthetic the Loom design called for, and a previous round of trying to retrofit Loom concepts on top of F-UI hit several quirks (window `M_HIDE` no-ops, listbox/combobox `M_GETSELECTED` handle inversion, `Tab M_SETINDEX` nested-iterator pathology) that made the retrofit a losing game. Loom paints everything itself through `Theme.bb` primitives directly on top of Blitz3D.

**Caveat for the future:** F-UI is still pulled in for things F-UI is genuinely good at — file dialogs, native text input boxes — if/when Loom needs them. Today Loom needs neither (it's read-only). When editing lands, expect F-UI to be Included for the text-input widgets only.

## Why no `ClientAreas.bb` Include

`ClientAreas.bb`'s `LoadArea` is the canonical 3D zone-mesh loader. It's transitively coupled to GUE's UI substrate: `GY_Cam` (Gooey's 3D camera), `GY_CreateProgressBar`, `ResolutionType`, `RandomImages`, `GetMusicName$`, `GetTexture`, `GetFilename$` (which is defined inside `GUE.bb` itself, not in a shared module). Pulling it in would lock Loom to GUE's UI substrate — exactly what Loom is supposed to decouple.

Concrete consequence: **Loom cannot render the 3D zone mesh.** Zone composer shows zone metadata as text + portal-target chips. See [decisions/004-deferred-3d-viewport.md](decisions/004-deferred-3d-viewport.md) for the path to fixing this in beta (extract `GetFilename$` to a shared helper; either rewrite `LoadArea`'s data path with the GUI side ripped out, or build a Loom-side mesh loader).

## Data-model gotchas

The thread model assumes reference fields are typed pointers, but rcce2 often stores them as plain strings:

- **Actor → faction**: typed (index into `FactionNames$`). Thread works.
- **Actor → anim set**: typed (`AnimSet\ID`). Thread works.
- **Zone → portal target**: stored as `PortalLinkArea$[i]` *string*. Composer resolves the string to a zone `Handle` via `Composer_FindZoneByName`; works for valid names, renders as broken-ref-red for unknown.
- **Faction → members**: not stored. Computed: walk `Each Actor` looking for `Ac\DefaultFaction = idx`. Cheap.
- **AnimSet → users**: same pattern — walk actors looking for M or F binding.
- **Item → script**: stored as `Item\Script$` string. No entity to jump to (scripts live as `.rsl` files on disk; not Loom's data model). Rendered as text.
- **Spell → script / emitter**: same. Strings. No thread.
- **Spell → casters**: cannot be derived from the data model — casts come from scripts. No back-reference possible without grepping scripts.

When adding a new thread chip site, check: is the reference field a typed ID, or a string? Strings either resolve through a lookup (like zone names) or render as text (no thread).

## File-naming and convention rules

- All Loom-specific modules under `src/Modules/Loom/`
- All exported symbols prefixed by their module: `Browser_*`, `Composer_*`, `Threads_*`, `LoomTheme_*`, `Loom_*` for cross-module state
- Color constants: `LOOM_STONE_900_R / _G / _B` (channel-separated so `LoomFill(x, y, w, h, ...)` calls don't need to unpack a single int)
- Layout constants module-prefixed: `BR_TOP_RIBBON`, `CMP_PAD`, `CHIP_PAD_X`
- New `.bb` files don't need to be Strict — non-Strict matches the existing module convention and avoids spurious cross-file declaration friction

## Known BlitzForge gotchas hit during the alpha

Documented here so the next agent doesn't re-discover them:

- **`..` line continuation is not supported.** Long function calls have to fit on one line. (`Mismatched brackets` is the parser error.)
- **`First` and `Last` are reserved.** Don't use them as variable names — `Expecting identifier near: first Got: first`. We use `firstFound`, `prev`, `endIdx`.
- **`data` is reserved** (the `Data/Read/Restore` family). Don't use as a variable name.
- **Single-line `For ... : If ... Then ... : Next` doesn't compose.** The `If ... Then ...` on the same line swallows the rest of the line up to the implicit `EndIf`, so the `: Next` becomes part of the IF body and the For has no Next. Always multi-line the body.
- **`New TypeName()` parens are required** in BlitzForge even with no args; bare `New TypeName` errors. (Holdover from the BlitzForge skill — if you forget, the parser complains.)
- **Type instances leak without `EnableGC`.** None of the Loom files use `EnableGC` (matches the project's canonical OO files — `Project Manager.bb` doesn't either). `BBList`'s `ListClear` and `ListRemove` only drop the list's references; the underlying instances stay on the heap. `Threads.bb` explicitly `Delete`s `LoomFocusEntry` instances in `back()` and `clearStack()` to avoid leaking N entries per N back/forward navigations.
- **`Strict` + reassigning a Method-scope `Local` from inside nested `If`/`For` blocks doesn't compile.** Error: `<varname> assignment should start with local, global or const modifier`. Reassigning at the same nesting level as the `Local` declaration is fine; reassigning from a deeper nested block (or from a sibling `Else If` branch after using it in an earlier branch) errors. Workaround: write to a **Field on the Type** instead (`self\latch = True` works at any depth). Loom hit this in `Browser::drawCardGrid`'s six-branch `If/Else If` chain and ended up refactoring to per-kind grid methods (`drawActorGrid`, `drawItemGrid`, …) — which turned out to be cleaner OO design anyway.
- **Tab gadget `M_SETINDEX` has a nested-iterator bug in F-UI.** Calling `FUI_SendMessage(TabMain, M_SETINDEX, 9)` doesn't reliably switch the visible tab when called from outside F-UI's own click path. The previous retrofit round documented this; Loom sidesteps it entirely by not using F-UI's Tab gadget.
