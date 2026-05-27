# Loom roadmap

Shipped, next, and deferred. Read this when picking what to build next.

## Shipped (alpha)

- **Skeleton** — `bin/Loom.exe` builds, Project Manager launches it, themed window opens, exits cleanly. ([#292](https://github.com/RydeTec/rcce2/pull/292), merged)
- **Theme primitives** — Loom design palette + 2D drawing helpers. Everything Loom paints goes through this layer.
- **Data loading** — same loaders GUE uses, in the same order; Loom reads through the same in-memory representation.
- **Entity browser** — boot surface; six categories (Actors / Items / Spells / Zones / Factions / Animation Sets); card grid auto-fits window width.
- **Composer panel** — per-kind property pages for all six entity kinds.
- **Thread chips + back stack** — reference fields render as clickable chips; click jumps + pushes; Esc walks back.
- **Faction and animset back-references** — composer computes "members" / "used by" rosters from `Each Actor` walks. ([#296](https://github.com/RydeTec/rcce2/pull/296))

## Next up (in rough order of leverage)

### 1. Command palette (Ctrl+K find-anywhere)

Type-to-search across every entity in the project. Press Ctrl+K from anywhere → modal opens → type substring → ranked results → Enter jumps. The browser tab walk is fine for known categories, but for *"where's that Goblin Shaman I edited yesterday"* the palette collapses 3 clicks into 4 keystrokes.

Implementation sketch: a new `Modules/Loom/Palette.bb` module with a modal renderer + result list. Reads the same data globals the browser does. Jump uses `Threads_Focus` (no back-stack push — palette navigation is "go to," not "follow a thread") or `Threads_Jump` if invoked from inside a composer view (debatable; settle this in the PR).

Estimated scope: 250-300 LOC. One PR.

### 2. Search-within-category

Browser tab bar gets a search box next to it. Type to filter the current category's cards by name substring. Live filter, no Enter required. Independent of the global palette — solves *"this tab has 200 actors, I want the human ones"*.

Estimated scope: 100-150 LOC. Pairs naturally with #1 or ships first.

### 3. Editing — phase 1: free-form text + numbers

Click a composer row's value → it becomes editable. Esc cancels, Enter (or click-away) commits to the in-memory instance and sets a dirty flag. Save button persists to disk via the existing `SaveActors / SaveItems / …` functions GUE already exports.

Needs:
- A dirty-flag mechanism (per-kind, like GUE's `ItemsSaved` / `ActorsSaved` globals — Loom should write to those same globals so GUE notices Loom's edits)
- F-UI text input pulled in (the one place F-UI earns its keep — see [decisions/001-custom-draw-not-fui.md](decisions/001-custom-draw-not-fui.md))
- Per-field validators (some fields are bounded ints, some are required, some have format constraints)
- A "Save" affordance in the chrome — probably top-right, mirroring the brass-rule placement

Estimated scope: 600-800 LOC. The big one. Likely needs to split across 2-3 PRs.

### 4. Editing — phase 2: reference-field editing

Once free-form fields edit, reference-typed fields (faction, anim set, zone target) need pickers. The chip becomes click-to-jump-OR-shift-click-to-edit (or some affordance — UX decision). Reuses the palette from #1 as the picker.

Estimated scope: 200-300 LOC. Builds on #1 and #3.

### 5. World atlas (spatial zone view)

Today the Zones tab in the browser is a card grid. The design called for a spatial atlas: zones laid out by position with portals drawn as lines between them. Worth shipping once there are enough zones in a project for the spatial layout to communicate something the grid doesn't (probably >10 zones).

Open question: zones in rcce2 don't have a stored "world position" — they're just floating in a flat list. The spatial layout would have to be either (a) derived from portal-link graph topology, or (b) a manual layout Loom remembers per-project.

Estimated scope: 400-500 LOC including layout heuristic. Skip if no project pressure for it.

### 6. Session timeline scrubber

Visible history of the session's edits (entity X changed field Y from A to B at time T). Click an entry → revert. Drag the handle → preview a past state. Needs an undo log that today's read-only alpha has no use for.

Builds naturally on #3 once dirty tracking exists. Pre-#3 there's nothing to scrub.

Estimated scope: 300-400 LOC. Don't start before #3.

## Deferred (with reasons — read before reopening)

### Literal 3D zone viewport

**Reason:** `ClientAreas.bb`'s `LoadArea` is entangled with GUE-specific UI globals — `GY_Cam`, `GY_CreateProgressBar`, `ResolutionType`, `RandomImages`, `GetMusicName$`, `GetTexture`, `GetFilename$` (which lives inside `GUE.bb` itself, not in a shared module). Pulling it into Loom would lock the two editors together at the UI layer — defeating the entire point of Loom as a parallel editor.

**Path to unblocking:** Either (a) extract the data-only parts of `LoadArea` into a new `Modules/AreaMeshLoader.bb` that both GUE and Loom can call, or (b) write a Loom-side mesh loader that parses the same `.dat` format independently. (a) is correct, (b) risks drift. Either is a meaningful refactor — probably needs to be its own multi-PR project before Loom can pick it up.

See [decisions/004-deferred-3d-viewport.md](decisions/004-deferred-3d-viewport.md) for full context.

### Validation conscience ribbon

**Reason:** The design called for a top status ribbon with broken-reference counts, balance hints, and unsaved-entity badges. Loom already detects broken references inline (thread chips render in red when they don't resolve). A separate ribbon adds value when there are *many* findings and you need a roll-up — that's mostly a #3-editing concern (unsaved counts) plus a real validator framework (balance hints).

**Path to unblocking:** ships after #3 (editing), since pre-edit there are no unsaved entities to count.

### Walk-in playtest

**Reason:** The design called for "spawn into the zone as a player without restarting the server." This requires a live server bridge — Loom would have to either (a) speak the wire protocol to a running `Server.exe` to inject a player, or (b) embed a server in-process. (b) is huge. (a) needs the wire protocol stable and a way for the server to accept "spawn this account at this position" out of band, which it doesn't today.

**Path to unblocking:** server-side feature work, not Loom work. Out of scope until a Server.exe admin/test API exists.

### Aesthetic immersion toggle (Tool / Balanced / In-world)

**Reason:** The design had a slider for chrome-density: utilitarian "Tool" mode at one end, fully-immersive "In-world" parchment-scroll aesthetic at the other. Cute, but not load-bearing for the alpha. The current Loom chrome is "Balanced" by default and there's no demand yet for variants.

**Path to unblocking:** ship when there's a real user request. Don't pre-build.

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
