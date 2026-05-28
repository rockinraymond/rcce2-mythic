# Loom roadmap

Shipped, next, and deferred. Read this when picking what to build next.

## Shipped (alpha + beta)

- **Skeleton** — `bin/Loom.exe` builds, Project Manager launches it, themed window opens, exits cleanly. ([#292](https://github.com/RydeTec/rcce2/pull/292), merged)
- **Theme primitives** — Loom design palette + 2D drawing helpers. Everything Loom paints goes through this layer.
- **Data loading** — same loaders GUE uses, in the same order; Loom reads through the same in-memory representation.
- **Entity browser** — boot surface; six categories (Actors / Items / Spells / Zones / Factions / Animation Sets); card grid auto-fits window width.
- **Composer panel** — per-kind property pages for all six entity kinds.
- **Thread chips + back stack** — reference fields render as clickable chips; click jumps + pushes; Esc walks back.
- **Faction and animset back-references** — composer computes "members" / "used by" rosters from `Each Actor` walks. ([#296](https://github.com/RydeTec/rcce2/pull/296))
- **Editing phase 1 (free-form text)** — Spell name editable; per-tab dirty flags shared with GUE; Save button + serializer dispatch. ([#334](https://github.com/RydeTec/rcce2/pull/334))
- **Command palette (Ctrl+K)** — modal find-anywhere across all entity kinds with prefix/substring scoring, arrow-key navigation, click-or-Enter to jump. ([#335](https://github.com/RydeTec/rcce2/pull/335))
- **Search-within-category** — live filter input above the card grid; case-insensitive substring match against the current category's name field. Esc clears the filter (priority above the back-stack pop). ([#335](https://github.com/RydeTec/rcce2/pull/335))
- **Editing phase 1 (broaden)** — int / float / bool field types; ~40 editable fields across every entity kind (Actor / Item / Spell / Zone / Faction / AnimSet); Save dispatch for every kind (`SaveActors`, `SaveItems`, `SaveFactions`, `SaveAnimSets`, `ServerSaveArea`); `SetFactionName` helper added to `Actors.bb` to work around the Strict-mode global-array-write trap. ([#336](https://github.com/RydeTec/rcce2/pull/336))
- **Entity creation (+ New)** — `EntityFactory.bb` wraps the GUE constructors (`CreateActor`, `CreateItem`, `CreateSpell`, `ServerCreateArea`, `CreateAnimSet`, faction-slot-first-empty); "+ New X" button on the browser filter bar dispatches per active category, focuses the new entity for immediate editing, marks the kind dirty. Zone names auto-deduplicate via `EntityFactory_UniqueZoneName` so a new zone doesn't overwrite an existing `.dat`. ([#337](https://github.com/RydeTec/rcce2/pull/337))
- **Entity deletion + Discard + Validation Ribbon** — Delete button on the composer (two-click arm/confirm); Discard button (revert kind from disk, also arm/confirm); Validation Conscience Ribbon at the top of every Loom surface showing per-kind dirty badges (click to save), broken-reference count (Actor->Faction, Actor->AnimSet, Zone->Portal->Zone), and total entity counts. New `Ribbon.bb` module; new `DeleteX Template` helpers in `Actors.bb`/`Items.bb`/`Spells.bb`/`Animations.bb` (non-Strict, to work around the Dim-write trap). ([#338](https://github.com/RydeTec/rcce2/pull/338))
- **World Atlas** — design's #3 signature surface. Zones tab gains a Card / Atlas toggle. Atlas renders zones as nodes with portals as edges using a Fruchterman-Reingold force-directed layout derived from the portal-link graph topology. Click a node → focus that zone; layout rebuilds on zone add / delete; circular seeding avoids the all-same-position singularity. ([#339](https://github.com/RydeTec/rcce2/pull/339))
- **Reference-field editing (phase 2)** — right-click any thread chip → palette opens as a picker filtered to that chip's kind; choosing writes the new refID into the underlying field (Actor→DefaultFaction, Actor→MAnimationSet, Actor→FAnimationSet, Zone→portal target by name). Works on broken-ref chips too (so dangling references can be repaired in place). Picker mode with empty query lists every candidate of the kind (no need to type to discover the roster). ([#340](https://github.com/RydeTec/rcce2/pull/340))
- **Session Timeline Scrubber** — design's #5 signature surface. `Modules/Loom/Timeline.bb` records every in-memory mutation (edit / toggle / create / delete) into a ring buffer capped at 200 entries. Ctrl+H opens a modal showing entries newest-first with timestamp / kind / entity / field / before→after / click-to-revert (edits + toggles only; creates and deletes log but don't revert). Module-level recorder facade (`Timeline_Record*`) so Composer / EntityFactory / Palette can record without an instance ref. ([#341](https://github.com/RydeTec/rcce2/pull/341))
- **Broken-ref finder modal** — extends the Conscience Ribbon's broken-ref count from a passive number into a clickable chip. New `Modules/Loom/BrokenRefs.bb` enumerates each broken reference (Actor→Faction/AnimSet, Zone→portal-target) with diagnosis text and click-to-jump to the source entity. Capped at 250 entries so a fundamentally broken project doesn't render thousands of rows. ([#342](https://github.com/RydeTec/rcce2/pull/342))
- **Browser keyboard navigation** — arrow keys move a brass-ringed selection cursor across the active category's card grid; Enter focuses the selected card. Up/Down jumps by row width (`lastCols`), Left/Right by 1. Selection clamps on category switch. Ctrl-anything skipped so global shortcuts (Ctrl+K / Ctrl+H) don't dribble through. ([#342](https://github.com/RydeTec/rcce2/pull/342))
- **Tools tab** — new browser category exposing GUE's seven standalone editor launchers (RC Architect, Terrain / Caves / Rock / Tree Editor, Gubbin Tool, Spell Wizard). Each card shows the tool name, description, and a Launch >> hint; click `ExecFile`s the .exe with the project's `Data/` folder as CWD. Missing-binary detection paints the card with a danger-red border + "binary not built" label so the user gets immediate feedback when the partial-build trap bites. ([#343](https://github.com/RydeTec/rcce2/pull/343))
- **Recents list (Ctrl+R)** — persisted per-project list of recently-focused entities. `Modules/Loom/Recents.bb` writes to `Data/Loom/recents.txt` (via `SafeWriteOpen/Commit`); load on boot, save on shutdown. `Threads::focus` + `Threads::jump` emit through a `Recents_Record` facade. Stable keys per kind (refID for typed entities, Name for zones since Handles regenerate) so the list survives across sessions. Ctrl+R opens a modal newest-first; stale entries (referenced entity deleted between sessions) render in danger-red and skip on click. ([#344](https://github.com/RydeTec/rcce2/pull/344))
- **World-state cache (perf)** — new `Modules/Loom/WorldCache.bb` holds cached aggregates (broken-ref count, 6 entity totals). Before: `Ribbon::recomputeCache` walked `O(actors * animsets + zones * portals * zones)` every frame. After: the same scan fires at most once per mutation (Composer commit/toggle/discard, EntityFactory create/delete, Palette picker commit invoke `WorldCache_Invalidate`); between mutations the per-frame ribbon paint is constant-time. Same recorder-facade pattern as Timeline / Recents (ADR 005). ([#346](https://github.com/RydeTec/rcce2/pull/346))
- **Save All (Ctrl+S) + save-on-exit prompt** — new `Modules/Loom/SaveAll.bb` dispatches `Composer::commitSaveForKind` for every dirty kind in one shortcut. Esc-exit-empty now intercepts when any kind is dirty and opens an `ExitPrompt` modal with Save All / Discard All / Cancel buttons; Esc inside the modal = Cancel (panic-press doesn't lose work). Without this a session with edits across four tabs needed four clicks, and Esc-exit silently discarded unsaved changes. ([#347](https://github.com/RydeTec/rcce2/pull/347))
- **Visual polish** — fixed UTF-8 `·` → Latin-1 `Â·` encoding artifact across 14 sites (replaced with ASCII `|`). Added subtle vertical gradients to all 7 flat chrome bands (conscience ribbon, brand strip, tab bar, filter bar, footer, composer panel, each card). Thickened card top accent from 1px → 3-line brass stack. Loaded Segoe UI as the body font with graceful fallback. New `LoomTheme_UseBody` / `LoomTheme_UseDisplay` font-swap helpers. ([#348](https://github.com/RydeTec/rcce2/pull/348))
- **Help (F1) + display-font adoption** — new `Modules/Loom/Help.bb` modal listing every keybinding + mouse interaction in a Global / Browser / Composer / Ribbon table. Adopted `LoomTheme_UseDisplay` for the LOOM brand mark + every modal title (Palette / Timeline / Recents / BrokenRefs / ExitPrompt / Help itself). Footer hint gains "F1 for shortcuts" so users can discover the cheat sheet without already knowing about it. ([#349](https://github.com/RydeTec/rcce2/pull/349))
- **Toasts + bigger section headers** — new `Modules/Loom/Toasts.bb` paints bottom-right transient notifications (3s TTL, capped at 5 stacked, color-coded success/info/warning/danger). Surfaces fire via `Toast_Show` facade. Hooked into every Composer save (success/failure), every EntityFactory create (success) + delete (danger), and SaveAll's summary. Composer's section headers swap to display-font + triple-rule brass ornament so each "Threads", "Restricted to", "Script" boundary reads as a deliberate division instead of a thin tick mark.

All six original "next up" roadmap items are shipped, plus eight scope-expanded surfaces (palette, search-within-category, entity creation, deletion, discard, conscience ribbon, broken-ref finder, keyboard nav, atlas, ref-field editing, timeline scrubber, recents, tools tab). Loom is now in **beta** — every primary GUE workflow has a Loom equivalent.

Beta vs alpha distinction: alpha was read-only browsing; beta covers full editing parity with GUE. Out-of-scope items (3D viewport, walk-in playtest, multi-cursor) remain in the deferred list below.

## Next up

The next-tier roadmap items, in rough order of leverage:

### 1. Bulk edit / multi-select

Select multiple cards via Shift+Click on the grid; opening the composer shows a unified property panel where edits apply to every selected entity. Useful for "bump every weapon's damage by 10%." Today the composer is single-focus only.

Estimated scope: 400-600 LOC. Touches Browser (selection set), Composer (multi-focus rendering), writeField (broadcast).

### 2. Asset preview (textures + meshes)

Item / Actor reference textures and meshes by integer ID. The composer shows the ID but not the asset itself. A small preview panel in the composer (or a hover-thumbnail on the chip) would surface the asset directly. Needs the texture / mesh loaders Loom currently avoids — likely a Loom-side decoder rather than pulling in GUE's media stack.

Estimated scope: 500-800 LOC. Significant new surface.

### 3. Aesthetic immersion toggle (Tool / Balanced / In-world)

The design's slider for chrome-density: utilitarian "Tool" mode at one end, fully-immersive "In-world" parchment-scroll aesthetic at the other. Current Loom chrome is "Balanced." Add a toggle in the Conscience Ribbon's right-side that re-themes the surfaces. Mostly a Theme.bb palette swap + a few layout tweaks per mode.

Estimated scope: 200-300 LOC.

## Deferred (with reasons — read before reopening)

### Literal 3D zone viewport

**Reason:** `ClientAreas.bb`'s `LoadArea` is entangled with GUE-specific UI globals — `GY_Cam`, `GY_CreateProgressBar`, `ResolutionType`, `RandomImages`, `GetMusicName$`, `GetTexture`, `GetFilename$` (which lives inside `GUE.bb` itself, not in a shared module). Pulling it into Loom would lock the two editors together at the UI layer — defeating the entire point of Loom as a parallel editor.

**Path to unblocking:** Either (a) extract the data-only parts of `LoadArea` into a new `Modules/AreaMeshLoader.bb` that both GUE and Loom can call, or (b) write a Loom-side mesh loader that parses the same `.dat` format independently. (a) is correct, (b) risks drift. Either is a meaningful refactor — probably needs to be its own multi-PR project before Loom can pick it up.

See [decisions/004-deferred-3d-viewport.md](decisions/004-deferred-3d-viewport.md) for full context.

### Walk-in playtest

**Reason:** The design called for "spawn into the zone as a player without restarting the server." This requires a live server bridge — Loom would have to either (a) speak the wire protocol to a running `Server.exe` to inject a player, or (b) embed a server in-process. (b) is huge. (a) needs the wire protocol stable and a way for the server to accept "spawn this account at this position" out of band, which it doesn't today.

**Path to unblocking:** server-side feature work, not Loom work. Out of scope until a Server.exe admin/test API exists.

### Multi-cursor / collaboration

**Reason:** The design's aspirational story included "two designers in the same world." This is months of work (sync protocol, conflict resolution, presence indicators) and the rcce2 user base is currently solo or small async teams.

**Path to unblocking:** out of scope indefinitely.

### Inheritance / templating for entities ("base goblin + 3 variants")

**Reason:** The design called for "change the base goblin's run animation and all three variants update." This needs a real entity-inheritance model in the data format, which rcce2 doesn't have. Today, every NPC is a flat record.

**Path to unblocking:** data-format change in the engine. Loom can't add this unilaterally.

## Decision-making

When adding to the roadmap or repositioning items, the rough priority signals are:

1. **Does it surface content the user already has?** (Browser pivot was high-priority for this reason — alpha was hiding the project's content.)
2. **Does it unlock a real workflow?** Hero flows: "tune a spell," "find a broken reference," "see what uses this faction."
3. **Does it match the design north star?** (Threads, search, walk-in.) Or is it scope creep into things the design didn't call for?
4. **What's the unblock cost?** A small change with infrastructure dependencies (editing → dirty tracking → save) is cheap-looking but expensive. Quote the dependency.
