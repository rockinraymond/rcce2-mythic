# Loom

A redesigned editor for Realm Crafter projects. Ships as `bin/Loom.exe` alongside `bin/GUE.exe`, launched from Project Manager. **Read this first** if you're picking up Loom work — it explains what Loom is *supposed to be*, separate from what it currently *is*.

## North star

Loom exists because GUE has aged into a 14-tab, dropdown-driven interface where the user is always one combobox-walk away from anything. The Loom redesign asked one question: *what would this look like if the editor's organizing principle wasn't tabs, but the relationships between the things the user is actually building?*

The answer the design landed on is:

> **Every reference between entities is a visible, clickable thread. Whatever you focus pulls to the center; its dependencies radiate outward. Edits propagate visibly. Broken refs fray.**

The hero flow this is designed around:

> "Build a forest clearing → author the goblin shaman that lives in it → tune the fireball it casts." One uninterrupted spatial session, no tab-flipping, no losing your place when you follow a reference.

The literal design artifact — the React/SVG prototype the user iterated through with the Claude Design tool — is preserved in [prototype/](prototype/). Open [prototype/Loom - World Editor.html](<prototype/Loom - World Editor.html>) in a browser to see and click through what Loom is meant to feel like at its most ambitious. **That prototype is the north star.** What ships in `bin/Loom.exe` today is one slice of it.

The full original design brief — written for an outside design agent — is preserved as the seed conversation in [prototype/design-session-transcript.md](prototype/design-session-transcript.md). It includes the user stories Loom is meant to serve and the future-state stories that aren't built yet.

## Six signature surfaces (from the design)

1. **Thread overlay** — every reference between entities renders as a glowing chip. Click jumps you to the target. Broken refs fray.
2. **Validation conscience** — top status ribbon showing world-health at a glance: unsaved count, broken references, balance hints.
3. **World atlas** — spatial overview of every zone, with portals drawn as lines between them.
4. **Command palette (Ctrl+K)** — type-to-search find-anywhere.
5. **Session timeline scrubber** — visible history of the current session's edits, with a draggable handle to rewind.
6. **Walk-in playtest** — spawn into the zone you're editing as a live player without restarting the server.

The shipped alpha implements **one of these (threads)** and a simpler version of one more (the atlas, as the browser's category grid). The other four are deferred — see [roadmap.md](roadmap.md).

## What Loom is today (alpha)

Read-only entity browser with thread navigation. Specifically:

- Boots into a **browser** that lists every actor / item / spell / zone / faction / animation set as clickable cards.
- Picking a card opens a **composer** panel on the right showing the entity's full property set.
- Reference fields render as **thread chips**: click an actor's faction → composer switches to that faction with its member roster → click a member → that actor. Esc walks back through the trail.
- Reads through the same data loaders GUE uses (`LoadActors`, `LoadItems`, `LoadSpells`, `ServerLoadArea`, …) so anything GUE can edit, Loom can see.

What it deliberately can't do:

- **Edit anything.** Every field is read-only. Editing needs save / dirty tracking that's its own design surface — see [decisions/002-read-only-alpha.md](decisions/002-read-only-alpha.md).
- **Render zones in 3D.** The 3D mesh loader is locked into GUE's UI substrate — see [decisions/004-deferred-3d-viewport.md](decisions/004-deferred-3d-viewport.md).
- **Search across everything (Ctrl+K).** Not yet wired. The command palette is on the roadmap.

## How to read this directory

| File | What it's for |
|---|---|
| [README.md](README.md) (this) | The north star + what's shipped. Start here. |
| [architecture.md](architecture.md) | Module map, data flow, where state lives, the rules a future change should respect. |
| [roadmap.md](roadmap.md) | Shipped / next-up / deferred (with reasons). The list a future agent uses to pick what to build next. |
| [decisions/](decisions/) | ADR-style records of the calls that shaped the alpha. Read these *before* arguing with a choice — the rationale is here. |
| [prototype/](prototype/) | The literal Claude Design bundle: HTML/JSX/CSS prototype + design system + screenshots + the original session transcript. **Reference, not buildable.** Treat as a visual spec. |

## How to actually run Loom

```
compile.bat       # builds bin/Loom.exe alongside GUE.exe / Server.exe / etc.
```

Then open Project Manager (`Project Manager.exe` in the repo root), pick a project, switch to the Engine tab, and click **Loom (Alpha)** next to **Game Unified Editor**.

The button auto-disables if `bin/Loom.exe` is missing.

## How to contribute (as a future agent or human)

1. **Read [architecture.md](architecture.md) first.** It explains why the code is shaped the way it is and what the existing modules expect of each other.
2. **Check [roadmap.md](roadmap.md).** If what you're about to build is "deferred for reason X," read reason X before reopening it.
3. **Browse [decisions/](decisions/).** Several seemingly-natural moves (rendering through F-UI, pulling in `ClientAreas.bb`, calling `M_SETINDEX` on a Tab) have already been tried and failed for documented reasons.
4. **Reference [prototype/](prototype/)** for visual decisions. The palette, the chip shape, the section header rhythm — when in doubt, match the prototype.

## What "good" looks like for a Loom change

- Surfaces the user's content, doesn't hide it behind ceremony
- Renders through `Theme.bb` primitives so the dark-fantasy look stays consistent
- Reads through GUE's existing data loaders so the two editors can't drift
- Adds a thread chip wherever an entity references another entity
- Doesn't `Include` any module that drags in GUE's UI substrate (`GY_*`, `app\overWin`, etc.)
- Compiles all five engine targets (Server / Client / Project Manager / GUE / Loom) — Loom is parallel to GUE, not at its expense
