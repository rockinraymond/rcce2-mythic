# RSL — the RealmCrafter Scripting Language

RSL is the language you write game logic in: NPC dialog, quests, shops, spell and
item effects, death and login hooks, slash-commands. Scripts are plain-text `.rsl`
files in `Data\Server Data\Scripts\`; the server compiles them at boot and runs
them in response to game events.

This page teaches the **language an author writes**. It is one of three scripting docs,
and they don't overlap:

| Doc | Answers |
|---|---|
| **This page** (`scripting/language.md`) | How do I *write* a script? Syntax, entry points, waits, privilege. |
| [`bvm-reference.md`](../bvm-reference.md) | What functions can I call? The full catalog of native `BVM_*` commands (auto-generated). |
| [`modules/scripting.md`](../modules/scripting.md) | How does the *engine* compile, dispatch, and garbage-collect scripts? (internals) |

Every example below is taken from a shipped script under `Data\Server Data\Scripts\`.

---

## 1. A script is a file of functions

```blitz
Using "RC_Core.rcm"
; Default server script -- used when no other script is specified.

Function Main()
	Return
End Function

Function Examine()
	Player = Actor()
	Target = ContextActor()
	Output(Actor(), "This is a " + Name(Target))
End Function
```
*(`Default.rsl`)*

- **`Using "RC_Core.rcm"`** pulls in the standard library — the high-level helpers
  (`OpenDialog`, `WaitKill`, `Output`, …) every script uses. Start every file with it.
  `RC_Core.rcm` is a *compiled* module that ships with the engine; its source is not in
  the project, so treat its helpers as a black-box API you call (this page documents the
  ones that matter; the rest are in [`bvm-reference.md`](../bvm-reference.md)).
- **`Function Name() … End Function`** declares a function. The engine calls specific
  function names on specific events (§5). Your own helper functions can have any name.
- **`;`** begins a line comment.
- **`Return`** exits the current function (optionally returning a value, §3).

Scripts are referenced **by name without the extension** — the file `Click_Merchant.rsl`
is the script `"Click_Merchant"`. Names are case-insensitive.

---

## 2. Variables and types

Variables are **declared implicitly** — assign to a name and it exists. A one-character
**sigil suffix** sets the type:

| Sigil | Type | Example |
|---|---|---|
| `$` | string | `PlayerName$ = Name(Player)` |
| `%` | integer | `MaxSlots% = 8` |
| `#` | float | `Dist# = 4.5` |
| *(none)* | integer | `Result = DialogInput(...)` |

A bare name (no sigil) is an integer. The same logical variable keeps its sigil
everywhere it's read **or** you can drop the sigil once it's been introduced — both
`QuestName$ = "..."` and a later `QuestStatus(Player, QuestName)` refer to the same
string. Keep a name's sigil consistent for readability.

**Identifiers are case-insensitive.** `Actor()` and `actor()`, `Player` and `player`
are the same — `Default.rsl`'s `Trade()` uses all-lowercase, `Examine()` uses
capitalised; both work.

Strings concatenate with `+`, and numbers auto-convert to text inside a concatenation:

```blitz
DialogOutput(Player, D, "You're carrying " + Gold(Player) + " gold.", 255, 255, 255)
```
*(`Click_Merchant.rsl`)*

---

## 3. Control flow

RSL uses the BlitzBasic family of control structures. All of these appear in shipped
scripts:

```blitz
If Result = 1
	; ...
ElseIf Result = 2
	; ...
Else
	; ...
EndIf

While ActorHasEffect(Target, "Poison")
	DoEvents(1000)
Wend

Repeat
	A = NextActorInZone(...)
	; ...
Until A = 0

For i = 1 To Count
	; ...
Next

Select Part2$
	Case "JAN"
		; ...
End Select
```

- `If` / `ElseIf` / `Else` / `EndIf` — note `EndIf` is one word, and `Then` is optional.
- `While` / `Wend` — loop while the condition holds.
- `Repeat` / `Until` — loop until the condition holds (the common actor-iteration shape,
  paired with `NextActorInZone`).
- `For` / `Next` — counted loop.
- `Select` / `Case` / `End Select` — multi-way branch.
- `Return` — leave the function; in a function with a sigil (`Function Foo$()`), `Return x`
  yields a value.

`DoEvents(milliseconds)` yields back to the engine for a beat — use it inside loops and
between dialog lines so the script cooperates with the server tick instead of blocking it.

---

## 4. Calling the engine

Everything a script *does* to the world is a built-in function call — `Output`,
`GiveItem`, `Attribute`, `Warp`, `FireProjectile`, `OpenDialog`, and ~220 more. Two are
special, because they tell you *who the script is about*:

- **`Actor()`** — the actor this run is *for* (the player who clicked, the caster, the
  attacker, …).
- **`ContextActor()`** — the secondary actor (the NPC that was clicked, the spell's
  target, …).

Exactly who those resolve to depends on the event that started the script — that's §5
and §6. The full function catalog (signatures + which ones need privilege) is
[`bvm-reference.md`](../bvm-reference.md); this page does not duplicate it.

---

## 5. Entry points: which function runs when

The engine starts a script by calling a **well-known function name** on a triggering
event. Give your script that function and it hooks the event. The table below is the
authoritative map (verified against the `ThreadScript(...)` dispatch sites in
`ServerNet.bb` and `GameServer.bb`):

| Event | Script | Function | `Actor()` | `ContextActor()` |
|---|---|---|---|---|
| NPC right-click → **Examine** | the NPC's `Script$`, else `Default` | `Examine` | the **clicker** | the **NPC** |
| NPC right-click → **Trade** | `Default` | `Trade` | the **clicker** | the **NPC** |
| NPC right-click (custom) | the NPC's `Script$` | `Main` | the **clicker** | the **NPC** |
| **Spell** cast | the spell's `Script$` | the spell's `SMethod$` | the **caster** | the **target** (0 if none) |
| **Item** used / eaten | the item's `Script$` | its `SMethod$`, else `Main` | the **user** | the **target** (0 if none) |
| **Attack** (combat) | `Attack` | `Main` | the **attacker** | the **target** |
| Player **Death** | `Death` | `Main` | the **dead player** | the **killer** (0 if none) |
| NPC death (spawn `DeathScript`) | the spawn's death script | `Main` | the **killer** (0 if none) | 0 |
| Player **LevelUp** | `LevelUp` | `Main` | the **player** | 0 |
| Player **Login** | `Login` | `Main` | the **player** | 0 |
| Area **entry / exit** | the area's `EntryScript$` / `ExitScript$` | `Main` | the **entering/exiting** actor | 0 |
| Chat **`/command`** (unrecognised) | `In-game Commands` | the **command name** | the **player** | 0 |

Two patterns are worth calling out:

- **`Default` is the fallback.** Examine/Trade (and right-click) run the NPC's own
  `Script$` if it has one, otherwise `Default.rsl`. That's why every project ships a
  `Default.rsl` with `Examine`/`Trade`.
- **Slash-commands dispatch by function name.** In `In-game Commands.rsl`, a player typing
  `/itempack` runs `Function ItemPack()`. The function name *is* the command.

---

## 6. `Actor()` vs `ContextActor()` — the clicker trap

For every NPC-interaction script (`Examine`, `Trade`, `RightClick`, item-on-target),
**`Actor()` is the player who clicked, and `ContextActor()` is the NPC** — not the other
way round. The dispatch is `ThreadScript(script, "Examine", Handle(clicker), Handle(NPC))`.

This is the single most important thing to get right:

```blitz
Function Examine()
	Player = Actor()         ; the player who right-clicked
	Target = ContextActor()  ; the NPC they clicked on
	Output(Actor(), "This is a " + Name(Target))
End Function
```
*(`Default.rsl`)*

It also matters for **security** (§7): because `Actor()` is the clicker, a hostile NPC's
script cannot use `Actor()` to act *on the clicker* with a privileged command — the
privilege gate blocks that path. Don't assume `Actor()` is the NPC.

Engine-tick events differ: for `Death`/`LevelUp`/`Login`/area scripts there is no clicker,
so `Actor()` is the subject (the dead/levelling/entering actor) and `ContextActor()` is 0
(except player `Death`, where `ContextActor()` is the killer). A spawn's `DeathScript`
is the exception worth memorising: its `Actor()` is the **killer**, not the NPC that died.

---

## 7. Privilege and the allowlist (read this before writing a reward script)

Many world-changing commands — `GiveItem`, `ChangeGold`, `SetAttribute`, `Warp`,
`KillActor`, `BanPlayer`, `SetActorTarget`, file/SQL/socket access, and more — are
**privileged**. A non-privileged script that calls one does **nothing** (it logs and
returns); it does *not* error. This is deliberate: it stops a hostile NPC's right-click
script from banning, robbing, or teleporting whoever clicked it.

Clicker-driven scripts (`Examine`, `Trade`, `RightClick`, item scripts) run
**non-privileged**. So a shop or quest-reward script that calls `GiveItem`/`ChangeGold`
will silently do nothing — **until you put it on the allowlist**:

> `Data\Server Data\Privileged Scripts.dat` — one script name per line. A script whose
> name is listed is elevated to privileged at spawn time. The server reads this file at
> boot, so **adding a name requires a server restart.**

`Click_Merchant.rsl` documents exactly this in its own header:

```blitz
; Reward BVMs are privileged -> this script is on Privileged Scripts.dat.
```

Two rules the engine enforces (see the gating section of the repo's `CLAUDE.md` for the
full threat model):

- **Elevation only, never demotion.** Being on the allowlist can *raise* a script to
  privileged; it never lowers one that was already privileged.
- **Engine-initiated spawns only.** The elevation fires only when the engine starts the
  script (a click, a cast, a death, …) — *not* when one script launches another via
  `ThreadExecute`. A hostile script can't borrow an allowlisted name to inherit its
  privileges.

If your NPC needs to grant gold/items/XP, add its script to the allowlist (and read the
whole script before you do — being on the list grants *all* privileged commands, not just
the ones you intended).

---

## 8. Waiting and dialogs

A script can **suspend** itself and resume later when something happens in the world —
this is how multi-step quests and interactive dialogs work. The helpers come from
`RC_Core.rcm`; the engine parks the script and wakes it when the condition fires.

### Quest waits

```blitz
Persistent(1)
; ...
ID = ActorID("Orc", "Raider")
WaitKill(Player, ID, 3)        ; suspend until the player has killed 3 Orc Raiders
; ...execution continues here once the third kill lands...
```
*(`Quest_OrcRaiders.rsl`)*

- **`WaitKill(actor, actorID, count)`** suspends until `actor` has killed `count` of the
  given actor type. Sibling waits: **`WaitSpeak(actor, target)`** (until `target` is
  right-clicked) and **`WaitItem(actor, name, qty)`** (until the inventory holds the item).
- **`Persistent(1)`** marks the script to survive the player logging out and back in —
  essential for a quest that spans a session. Without it, a logout ends the script.
- Time waits (`DoEvents`, timed waits) resume after a delay rather than on an event.

Under the hood the engine records the suspended script in a `PausedScript` list with a
reason code (`2`=WaitKill, `3`=WaitItem, `4`=WaitSpeak, `1`=logged-out) and re-enters it
when the condition is met — see `UpdateScripts` in `Scripting.bb` if you're curious, but
as an author you only need the helper above.

### Dialogs

```blitz
D = OpenDialog(Player, Target, "General Store")
DialogOutput(Player, D, "What can I get you?", 255, 255, 255)
Result = DialogInput(Player, D, "Potion - 25g|Sword - 85g|Just browsing", "|")
If Result = 1
	; first option chosen
ElseIf Result = 2
	; second option chosen
EndIf
CloseDialog(Player, D)
```
*(condensed from `Click_Merchant.rsl`)*

- **`OpenDialog(player, target, title)`** opens a dialog window and returns a handle `D`.
- **`DialogOutput(player, D, text, r, g, b)`** writes a coloured line.
- **`DialogInput(player, D, options$, separator$)`** shows the `separator`-delimited
  options and **suspends until the player picks one**, returning the **1-based index**
  of their choice (`1` for the first option). Branch on it with `If`/`ElseIf`.
- **`CloseDialog(player, D)`** dismisses the window.

> `OpenDialog`/`DialogInput`/`WaitKill` and friends live in the compiled `RC_Core.rcm`
> standard library, so you won't find their bodies in the project source. The contract
> above is what they guarantee to a script author; the engine-side packet handling that
> backs them is in `ServerNet.bb` / `ScriptingCommands.bb`.

---

## 9. A complete script, annotated

`Click_Merchant.rsl` is a full vendor — it ties together entry points, dialogs, the
privilege model, and control flow:

```blitz
Using "RC_Core.rcm"
; Reward BVMs are privileged -> this script is on Privileged Scripts.dat.

Function Main()                                  ; right-click entry point
	Player = Actor()                             ; the clicker (§6)
	Target = ContextActor()                      ; the merchant NPC

	D = OpenDialog(Player, Target, "General Store")
	DialogOutput(Player, D, "You're carrying " + Gold(Player) + " gold.", 255, 255, 255)
	Result = DialogInput(Player, D, "Potion of Healing - 25g|Just browsing", "|")

	If Result = 1
		If Gold(Player) >= 25                    ; afford check
			ChangeGold(Player, -25)              ; privileged -> needs allowlist (§7)
			GiveItem(Player, "Potion of Healing", 1)
			Output(Player, "You buy a Potion of Healing.", 0, 255, 0)
		Else
			Output(Player, "You can't afford that.", 255, 80, 80)
		EndIf
	EndIf

	DoEvents(1200)                               ; let the client settle
	CloseDialog(Player, D)
End Function

Function Examine()                               ; right-click "examine" entry point
	Output(Actor(), "A general store merchant. Right-click to buy potions and gear.")
End Function
```

The NPC's `Script$` field (set in the actor template / spawn) points at `Click_Merchant`,
so a right-click runs `Main`, and an examine runs `Examine`. Because `ChangeGold` and
`GiveItem` are privileged and this is a clicker-driven script, the merchant only works
because `Click_Merchant` is on `Privileged Scripts.dat`.

---

## Where to go next

- **The function catalog:** [`bvm-reference.md`](../bvm-reference.md) — every callable
  command, its parameters, and whether it needs privilege.
- **A guided tour of a real project's scripts:** [`sample-project-guide.md`](../sample-project-guide.md).
- **Engine internals** (compilation, `ScriptInstance` lifecycle, dispatch):
  [`modules/scripting.md`](../modules/scripting.md) and
  [`modules/scriptingcommands.md`](../modules/scriptingcommands.md).
