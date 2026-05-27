<!-- body { color:black background-color:white } a:link{ color:#0070FF } a:visited{ color:#0070FF } --> RealmCrafter: Community Edition Documentation

**Interface.bb**

The client-side 2D HUD substrate. Defines every per-window gadget handle (`W*`, `B*`, `L*`, `T*`, `S*`), the unified key-binding state (`Key_*` globals + `LoadControlBindings` / `SaveControlBindings` file I/O), the `InterfaceComponent` Type used by the persistent layout system (`LoadInterfaceSettings` / `SaveInterfaceSettings`), and the `ControlHit` / `ControlDown` / `ControlName$` cross-device input dispatch.

This module is **declaration-heavy, logic-light**. Most of it is `Global W<Window>`, `Global B<Button>`, `Global L<Label>`, `Dim` arrays for inventory / spell-book / action-bar slots, etc. — the file is the central registry of UI gadget handles, not the place where UI behavior lives. Per-window event handling lives in [`MainMenu.bb`](mainmenu.md), [`ClientNet.bb`](clientnet.md), and other consumers.

## Conceptual overview

### Three small Types

| Type | Purpose |
|---|---|
| `Dialog` | NPC-conversation modal. `Win`, 14 text lines (`TextLines[13]` / `TextText$[13]` + per-line R/G/B and `OptionNum[]`), an `ActorInstance` link, a `ScriptHandle`. Per-NPC dialog ID. |
| `TextInput` | Single-line text-entry modal. `Win`, `TextBox`, `AcceptButton`, `ScriptHandle`. Used by `BVM_GETSTRING` and similar script prompts. |
| `Bubble` | Floating chat-bubble entity that follows an actor for a short time. `EN`, `Width#` / `Height#`, `Timer`, `ActorInstance`. |
| `InterfaceComponent` | Persistent layout descriptor (X/Y/W/H in fraction-of-screen, Alpha, Component handle, Texture, R/G/B). Backs `Chat`, `ChatEntry`, `BuffsArea`, `Radar`, `Compass`, the per-attribute bar array, and inventory buttons. Read/written via `ReadInterfaceComponent` / `WriteInterfaceComponent`. |
| `EffectIcon` / `EffectIconSlot` | Buff-icon HUD entries (name, ID, texture). |

`Dialog` and `TextInput` are addressed by Handle via `DialogScriptHandle(Han)` from script context, which returns the `ScriptHandle` field for downstream BVM dispatch.

### The gadget-globals registry

The bulk of this file is one large registry of `Global` handles, grouped by HUD region:

| Group | Gadgets | Touched by |
|---|---|---|
| Action bar | `XPEN`, `BChat / BMap / BInventory / BSpells / BCharStats / BQuestLog / BParty / BHelp`, `BNextBar / BPrevBar`, `ActionBarSlots(35)`, `BActionBar(11)`, `ActionBarStart`, `ActionBarUpTex / DownTex` | UI input loop; `P_SetActionBar` packet handler |
| Character interaction window | `WCharInteract`, `SCharInteractHealth`, `LCharInteractTalk`, `CharInteract.ActorInstance`, `LCharInteractFaction / Level / Reputation` | NPC right-click / examine flow |
| Tooltip | `WTooltip` (created-on-the-fly), `WTooltipReturn`, `LTooltip` | hover detection in every window |
| Party | `WParty`, `BPartyLeave`, `PartyName$(6)`, `LPartyName(6)` | `P_PartyUpdate` packet handler |
| Menu (added 2014) | `WMenu`, `BMenu`, `BLogOut / BCharSelect / BExit / BOptions` | escape-key handler |
| Help | `WHelp`, `BHelp`, `SHelpScroll`, `HelpText$(99)`, `LHelp(14)` | `/help` chat command |
| Radar + map | `ShowRadar`, `WLargeMap`, `LargeMapVisible` (see [`Radar.bb`](radar.md) for the actual renderer) |
| Inventory | `WInventory`, `LInventoryGold`, `BInventoryDrop / Eat`, `WAmount` + amount-prompt support, `MouseSlot*` drag-state, `BSlots(Slots_Inventory)`, `WItemWindow` | inventory-window event loop |
| Trading | `WTrading`, `LTradingGold / Cost`, `BTradingOK / Cancel`, `TradeType`, `BCostUp / Down`, parallel mine/his slot arrays (32 each) | trade-window event loop; `P_InventoryUpdate "T"` |
| Char stats | `WCharStats`, attribute-name/value label arrays | `P_StatUpdate` consumer |
| Spells | `WSpells`, `BNextSpells / PrevSpells`, `LSpellsPage`, `WSpellRemove` + `WSpellError` modals, `BSpellImgs(9)` etc., `FirstSpell` cursor, `LastSpellRecharge` | `P_SpellUpdate` consumer |
| Quest log | `WQuestLog`, `BCompleteQuests / Next / Prev`, `FirstQuest`, `LQuestLines(16)` | `P_StandardUpdate` quest-entry consumer |
| Chat | `ChatHistory$(1999)` (permanent history), `ChatHistoryColour(1999)`, `ChatLines(0)` (resized on screen-resolution change), `MaxChatLine`, history mode toggle | `P_ChatMessage` consumer |

`ChatHistory$` is a 2000-slot ring; `ChatLines` is dynamically `Dim`-resized to whatever fits the current resolution. A future high-DPI / wide-screen aware UI overhaul would touch this `Dim ChatLines(N)` call site.

### Key bindings and the control-number space

`Key_*` globals store integer control codes; `LoadControlBindings` / `SaveControlBindings` persist them via plain `ReadInt` / `WriteInt` (no SafeWrite — the file is small and a corrupt control-bindings file resets to defaults harmlessly on next launch).

`ControlHit` / `ControlDown` / `ControlName$` partition the integer control space into three ranges:

| Range | Device | Implementation |
|---|---|---|
| `1..499` | Keyboard | Direct `KeyHit(Ctrl)` / `KeyDown(Ctrl)` pass-through. Keys numbered by Blitz3D's standard scan-code table. |
| `501..509` | Mouse | `MouseHit(1/2/3)` for buttons; `MXSpeed / MYSpeed / MZSpeed` axis tests for "Mouse Up/Down/Left/Right" and scroll-wheel up/down. |
| `1001..1008` | Joystick buttons | `JoyHit(Ctrl - 1000)`. |
| `1009..1016` | Joystick hat / axes | `JoyHat()` direction matching for "Hat Up/Down/Left/Right"; `JoyXDir() / JoyYDir()` for analog stick directions. |
| else | Unknown | `ControlName$` returns `LanguageString$(LS_Unknown)`. |

The hat-direction Cases use **edge detection** via static-`True` flags (`JoyHatUp = True` after the first hit, gating subsequent hits until the hat re-centers) — this is the **only** Hit-vs-Down distinction in the joystick path. Buttons get standard `JoyHit` edge behavior from Blitz; analog axes don't (they fire continuously while held).

`ControlName$` is a switch-statement of every supported control number to a human-readable label. Used to populate the key-binding UI in the options menu.

### Interface-component persistence

`LoadInterfaceSettings` / `SaveInterfaceSettings` (re)build the global `Chat / ChatEntry / BuffsArea / Radar / Compass / InventoryWindow / etc.` `InterfaceComponent` handles by reading/writing fixed-shape records. Save uses **atomic `SafeWriteOpen$` / `SafeWriteCommit%`** (lines 340-368) so an interrupted save can't leave a truncated layout file.

The component shape on disk is 8 fields per record:

```
WriteFloat X, Y, Width, Height, Alpha   ; 5 floats
WriteByte  R, G, B                       ; 3 bytes
```

`Chat` carries an extra `Texture` field (read/written separately as a `Short`); all other components are pure layout.

### `WordWrap(St$, MaxChars)`

Linear scan backward from `MaxChars` for the last space character — gives the split index for the next line in a word-wrapped paragraph. Returns `MaxChars` (mid-word break) if no space found in the search window. Returns `0` if the string is shorter than the limit (caller's signal: "no wrap needed").

This is a Blitz-friendly implementation of basic word-wrap — the `LTooltip` and dialog-multi-line rendering use it.

## Conventions for new code touching this module

- **A new UI window means a new `Global W<Name>` here**, paired with all `B<Name>` / `L<Name>` button/label handles, and ideally a `Dim` array if the window has repeating slots. Don't scatter window-state globals across consumer files.
- **`InterfaceComponent`-backed elements need read/write entries in `Load/SaveInterfaceSettings`** — otherwise the user's layout customization isn't persisted. The on-disk record order is positional (no length-prefix / tag), so **never reorder** the read/write sequence — match `Read*` order to `Write*` order.
- **`SaveInterfaceSettings` uses `SafeWriteOpen` / `SafeWriteCommit`.** If you add new persistent-layout state, follow the same atomic-write pattern (CLAUDE.md → "Atomic writes"). Direct `WriteFile` to the production path is forbidden.
- **`ControlHit` vs `ControlDown` returns slightly different shapes for joystick hat directions** (edge-detected via static `True` flags vs immediate true while held). Don't assume they're symmetric — use `ControlHit` for menu-navigation single-press, `ControlDown` for held-movement input.
- **Adding a new control number** requires entries in `ControlHit`, `ControlDown`, *and* `ControlName$`. Missing any one breaks UI rebinding or input dispatch.
- **`ControlName$` returns `LanguageString$(LS_Unknown)` for unrecognised control numbers** — never hard-code "Unknown" or similar strings; route through the [`Language.bb`](language.md) registry.

## Related modules

- [`Language.bb`](language.md) — `LS_*` constants for UI labels; `LanguageString$(LS_Unknown)` is the fallback for `ControlName$`.
- [`Logging.bb`](logging.md) — provides the `SafeWriteOpen$` / `SafeWriteCommit%` atomic-write helpers used by `SaveInterfaceSettings`.
- [`MainMenu.bb`](mainmenu.md) — heaviest consumer; the in-game UI event loop dispatches on the gadget globals declared here.
- [`Gooey.bb`](gooey.md) — provides the underlying Gooey UI primitives (`CreateWindow`, `CreateLabel`, `CreateListBox`, `CreateButton`, etc.). The `W*` / `B*` / `L*` handles here are Gooey-allocated.
- [`F-UI.bb`](../../src/Modules/F-UI.bb) — the alternate Float-UI system used by [`MediaDialogs.bb`](mediadialogs.md). Coexists with Gooey for editor tools.
- [`Radar.bb`](radar.md) — the `Radar.InterfaceComponent` declared here describes the **placement** of the radar HUD overlay; the actual renderer lives in `Radar.bb`.
- [`Interface3D.bb`](interface3d.md) — the 3D-world HUD pieces (chat bubbles in world space, name labels above actor heads) that complement the 2D UI here.

## See also

- CLAUDE.md → "Atomic writes" — the canonical `SafeWriteOpen` / `SafeWriteCommit` pattern.
- [`P_ChatMessage.md`](../protocol/packets/P_ChatMessage.md) — wire layer feeding `ChatHistory$()` / `ChatLines()`.
- [`P_StatUpdate.md`](../protocol/packets/P_StatUpdate.md) — wire layer feeding the per-attribute display bars in `AttributeDisplays(39)`.
- [`P_InventoryUpdate.md`](../protocol/packets/P_InventoryUpdate.md) — wire layer feeding `BSlots(Slots_Inventory)` and the trading parallel-slot arrays.

* * *

This module's source is mostly handle declarations with a handful of small functions. The 11 functions are: `DialogScriptHandle`, `LoadControlBindings`, `SaveControlBindings`, `WriteInterfaceComponent`, `ReadInterfaceComponent`, `LoadInterfaceSettings`, `SaveInterfaceSettings`, `WordWrap`, `ControlHit`, `ControlDown`, `ControlName$`. Read the source at [`src/Modules/Interface.bb`](../../src/Modules/Interface.bb) for full signatures.
