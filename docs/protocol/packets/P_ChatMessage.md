# P_ChatMessage

**Direction:** Both (C→S "I'm typing into chat"; S→C "render this colored line")
**Numeric ID:** 16
**Server handler:** [ServerNet.bb:183](../../../src/Modules/ServerNet.bb#L183)
**Client handler:** [ClientNet.bb:1213](../../../src/Modules/ClientNet.bb#L1213)

## Purpose

The catch-all text channel. Every user-typed chat line, every slash command, every server-side notification, every BVM `Output(...)` script line travels on this opcode. The client only renders; the server parses, applies access gates, fans out to the right audience (area / online / party / guild / single-target), and may dispatch script work as a side effect.

There is **no sub-code byte at the start of the wire payload**. The whole packet is a free-form string. Discrimination happens via two parallel signals:

- **First-character semantics on C→S**: `/` and `\` enter the slash-command parser; everything else is general chat.
- **First-byte colour escape on S→C**: bytes in `[250..254]` are rendered as colour escapes; anything else is normal-text body.

## Field layout

### C → S — "Player typed text"

| Offset | Width | Type | Field | Notes |
|---|---|---|---|---|
| 1 | N | String | Raw text the user typed | No length prefix; `M\MessageData$` is the whole payload. |

The handler bails immediately if `Len(M\MessageData$) <= 0 Or AI = Null` ([ServerNet.bb:185](../../../src/Modules/ServerNet.bb#L185)). After that:

- `Left$(M\MessageData$, 1) = "/" Or Left$(M\MessageData$, 1) = "\"` → strip the leading char, split on first space, `Command$` = `Upper$(...)`, dispatch into the giant `Select Command$` ([ServerNet.bb:187-693](../../../src/Modules/ServerNet.bb#L187)).
- Otherwise → general chat: prepend `"<" + AI\Name$ + "> "`, broadcast over the per-area `FirstInZone` chain, log to `ChatLog` per `ChatLoggingMode` ([ServerNet.bb:694-712](../../../src/Modules/ServerNet.bb#L694)).

### S → C — "Render this line with colour"

| Offset | Width | Type | Field | Notes |
|---|---|---|---|---|
| 1 | 1 | Byte | Colour escape (optional) | One of `250..254`, or any other byte → start-of-normal-text. |
| 2 | 3 | Byte×3 | R, G, B | **Only present when offset 1 = 250.** Each in `[0..255]`. |
| 2 or 5 | N | String | Message body | Rendered through `Output(...)` or `BubbleOutput(...)`. |

Colour-escape semantics ([ClientNet.bb:1215-1227](../../../src/Modules/ClientNet.bb#L1215)):

| Prefix | RGB | Channel / use |
|---|---|---|
| `Chr$(254)` | `255, 255, 0` (yellow) | System / GM / party-join / time-of-day / invite — server-authored notifications. |
| `Chr$(253)` | `255, 50, 50` (red) | Yell, error, ignore confirmation, ability-not-recharged. |
| `Chr$(252)` | `200, 10, 200` (magenta) | `/me` emote, `/pm` private-message, NPC→player chat ("Player: ..."). |
| `Chr$(251)` | `20, 220, 50` (green) | Guild chat, party chat. |
| `Chr$(250)` | (next 3 bytes) | Custom RGB — emitted by `BVM_OUTPUT(actor, text, R, G, B)` at [ScriptingCommands.bb:2793](../../../src/Modules/ScriptingCommands.bb#L2793). |
| _other_ | (default) | Normal text. If body starts `"<NAME> "` and `UseBubbles > 1`, renders as a 3D chat bubble above the named actor. |

The chat-bubble path ([ClientNet.bb:1231-1254](../../../src/Modules/ClientNet.bb#L1231)) only fires for the no-escape case — coloured lines never get bubbles. Bubble lookup goes through `FindPlayerFromName(...)`; if no actor matches, the line falls back to normal text output.

## C → S slash-command catalog

The dispatch is a single `Select Command$` keyed on `LanguageString$(LS_SC*)` ([Language.bb](../../../src/Modules/Language.bb)) — every command is localizable. Below uses the English defaults.

| Command | Gate | Effect |
|---|---|---|
| `/kick <name>` | DM | Sends `RCE_PlayerKicked` + `P_KickedPlayer` to target. |
| `/ignore <name>` / `/unignore <name>` | none | Mutates the inviter's `Account\Ignore$` CSV. |
| `/me <text>` | none | Area broadcast with `Chr$(252) + "* " + Name + " " + text`. |
| `/yell <text>` | none | Server-wide broadcast (walks `FirstOnlinePlayer` chain). |
| `/g <text>` | guild member | Walk `FirstOnlinePlayer`, send to same-TeamID. |
| `/p <text>` | party member | Walk `Party\Player[0..7]`. |
| `/pm <name>, <text>` | none | Walk `FirstOnlinePlayer`, deliver to first matching name. |
| `/invite` / `/accept` / `/leave` | none | Party state machine. |
| `/pet <name>, <cmd>, <args>` | own a pet | Walk `AI\FirstSlave` chain; dispatch `CommandPet(...)`. |
| `/trade <name>` | none | Player→player trade offer. |
| `/players` / `/allplayers` | none | Counter — current area vs. server-wide. |
| `/time` / `/date` / `/season` | none | Game-clock report. |
| `/warp <area>, <x>, <z>` | DM | Server-side warp. |
| `/warpother <name>, <area>, <x>, <z>` | DM | Warp another player. |
| `/xp <amount>` | DM | `GiveXP(AI, n)`. |
| `/gold <amount>` | DM | Direct gold adjustment + `P_GoldChange`. |
| `/setattribute <attr>, <n>` / `/setattributemax <attr>, <n>` | DM | Calls `UpdateAttribute` / `UpdateAttributeMax` for Health/Speed/Energy, otherwise writes through and broadcasts `P_StatUpdate`. |
| `/script <name>, <func>` | DM | Spawns a `ThreadScript(...)` with `privileged=1` — the script can call any privileged BVM. |
| `/gm <text>` | DM | Broadcast to all DMs only. |
| `/ability` / `/give` / `/weather` / `/netdump` | DM | Misc DM tools. |
| _other_ | none | Falls through to the `ScriptExists%("In-game Commands")` hook → `ThreadScript("In-game Commands", Command$, ..., Params$)`, otherwise replies `"Unknown command:"` ([ServerNet.bb:688-692](../../../src/Modules/ServerNet.bb#L688)). |

`/help [topic]` ([ServerNet.bb:40-108](../../../src/Modules/ServerNet.bb#L40)) is special: it emits one `P_ChatMessage` per line of help text, all prefixed `Chr$(254)`. The DM-only block at [ServerNet.bb:57-58](../../../src/Modules/ServerNet.bb#L57) is gated by the same `Account\IsDM` check.

## Validation requirements

### C → S handler ([ServerNet.bb:183-712](../../../src/Modules/ServerNet.bb#L183))

The packet body is free-form, so length / shape gates are minimal. The defences live in the per-command logic:

1. **Sender validity**: `AI = FindActorInstanceFromRNID(M\FromID)`; bails on `Null`.
2. **Non-empty body**: `Len(M\MessageData$) > 0`.
3. **Account-Null discipline**: every DM-gated command does `A.Account = Object.Account(AI\Account) : If A <> Null And A\IsDM = True`. The `A <> Null` check is load-bearing — a mid-logout account that has been `Delete`d but whose `Handle` is still on `AI\Account` returns `Null` from `Object.Account(...)`, and bare `A\IsDM` would crash the server **from a chat command** (see audit-comment at [ServerNet.bb:200-202](../../../src/Modules/ServerNet.bb#L200)). Apply this to **every new `/command` that needs a DM gate** — the pattern is non-negotiable.
4. **`/pet` chain walk**: walks `AI\FirstSlave / NextSlave` ([ServerNet.bb:266-273](../../../src/Modules/ServerNet.bb#L266)) — the per-leader chain that PR [#287](https://github.com/RydeTec/rcce2/pull/287) introduced, replacing a global `Each ActorInstance` scan.
5. **`/yell` / `/g` / `/gm` / `/pm` / `/allplayers` chain walks**: walk `FirstOnlinePlayer / NextOnlinePlayer` ([ServerNet.bb:408-462](../../../src/Modules/ServerNet.bb#L408)) — the engine-wide players chain (PR [#288](https://github.com/RydeTec/rcce2/pull/288) era). Filters that used to require `If A2\RNID > 0` are gone because the chain only contains online players.
6. **Mid-warp `AreaInstance` guard**: `/me`, `/players`, and the general-chat fallback all do `AInstance.AreaInstance = Object.AreaInstance(AI\ServerArea) : If AInstance <> Null Then ...` — soft-fails when the actor is in the brief window between `SetArea` rebinding zones. The chat line just doesn't broadcast that tick; no crash.
7. **PlayerIgnoring filter**: every fan-out skips recipients whose ignore-list contains the sender ([ServerNet.bb:394](../../../src/Modules/ServerNet.bb#L394) / [410](../../../src/Modules/ServerNet.bb#L410) / [457](../../../src/Modules/ServerNet.bb#L457)).

### Privilege classification

The DM gate is **not** the BVM `RequirePrivileged()` gate. Slash commands are dispatched directly from the packet handler — they never enter the BVM, so `BVM_RequirePrivileged` is irrelevant here. The `Account\IsDM` boolean is the only check. Two notes:

- **`/script` spawns with `privileged=1`** ([ServerNet.bb:385](../../../src/Modules/ServerNet.bb#L385)) — this is the *only* path from chat that lets a script call privileged BVMs (Ban/Kick/Warp/CreateUDPStream/etc.). The DM gate is what authorises the elevation; downstream BVMs check the privileged bit on `ScriptInstance`.
- **Unknown commands fall through to `"In-game Commands"`** ([ServerNet.bb:688-692](../../../src/Modules/ServerNet.bb#L688)) — these scripts are spawned with the default (un-privileged) flag. Content authors who want player-facing slash commands should put them there; only the engine's DM-only set should ever reach the `privileged=1` path.

## S → C emit pattern

Every `RCE_Send(Host, target_rnid, P_ChatMessage, Pa$, True)` follows the same shape: the colour-escape byte (or none) is the first character of `Pa$`. Recipients:

- **Self-only** notifications (errors, confirmations, `/players` results) — sent to `AI\RNID`.
- **Area broadcast** (`/me`, general chat) — walks `AInstance\FirstInZone` chain, sends per actor with `RNID > 0`.
- **Server-wide broadcast** (`/yell`, `/gm`, `/g`) — walks `FirstOnlinePlayer` chain.
- **Party broadcast** (`/p`, party-join notification) — `For i = 0 To 7 : Party\Player[i]` array.
- **Single target** (`/pm`, `/invite`, `/trade`) — direct send to the resolved target's `RNID`.

The general-chat fallback at [ServerNet.bb:705-710](../../../src/Modules/ServerNet.bb#L705) also adds the line to the server's `Game\ChatText` ListBox (for the dedicated-server console) and writes it to `ChatLog` when `ChatLoggingMode > 0`.

## Anti-cheat surface

`P_ChatMessage` is not the high-stakes packet `P_AttackActor` is, but it has the largest privilege surface of any packet because of `/script`. The defences:

- **Wire payload is text, not opcode** — a malformed packet can produce a garbled chat line but cannot reach `/script` unless the sender has `IsDM` on their account.
- **DM bit is server-side only** — clients cannot fabricate `IsDM`; it lives on the `Account` row in MySQL / `accounts.dat`.
- **Privileged spawn is gated on DM bit** — losing DM in the middle of `/script` would not retroactively un-privilege a running script; the bit is captured at spawn. That's fine — administering a DM-set should be done in a separate channel anyway.
- **Soft-fail on every Account-Null path** — the `A <> Null` checks ensure a mid-logout DM cannot crash the server by typing into chat as the account is being freed.

## Historical bugs / PR references

| PR | Fixed |
|---|---|
| Audit pre-PR | `Account\IsDM` reads after `Object.Account(...)` did not check Null first — server-crash via slash command from a mid-logout DM. Audit-comments at [ServerNet.bb:200-202](../../../src/Modules/ServerNet.bb#L200) / [220-221](../../../src/Modules/ServerNet.bb#L220) / [237](../../../src/Modules/ServerNet.bb#L237) record the pattern. |
| [#287](https://github.com/RydeTec/rcce2/pull/287) | `/pet` walks the per-leader `FirstSlave` chain (replacing global `Each ActorInstance` scan). |
| [#288](https://github.com/RydeTec/rcce2/pull/288) and predecessors | `/yell` / `/g` / `/gm` / `/pm` / `/allplayers` walk the `FirstOnlinePlayer` chain. |
| PRs [#154](https://github.com/RydeTec/rcce2/pull/154) / [#155](https://github.com/RydeTec/rcce2/pull/155) / [#176](https://github.com/RydeTec/rcce2/pull/176) / [#182](https://github.com/RydeTec/rcce2/pull/182)–[#188](https://github.com/RydeTec/rcce2/pull/188) | `Object.AreaInstance(...)` Null discipline sweep — covers `/me`, `/players`, and general-chat fallback paths. |
| Unknown-command notify | Pre-fix, an unknown slash command silently no-op'd. Now replies `"Unknown command: /xxx. Type /help for a list."` ([ServerNet.bb:691](../../../src/Modules/ServerNet.bb#L691)). |

## Related packets

- [`P_StandardUpdate`](P_StandardUpdate.md) — `Chr$(254)` system lines often follow a warp; the warp itself rides `P_StandardUpdate`.
- [`P_AttackActor`](P_AttackActor.md) — `/kick` is the chat-equivalent of forcibly removing an actor; combat is the other way out.
- [`P_GoldChange`](../index.md) — `/gold` emits this directly; the chat line is just confirmation.
- [`P_KickedPlayer`](../index.md) — paired with `/kick` to actually disconnect the target.

## See also

- [`../encoding.md`](../encoding.md) — wire-encoding primitives.
- [`../handler-conventions.md`](../handler-conventions.md) — soft-fail discipline, Account-Null pattern, FirstOnlinePlayer / FirstInZone / FirstSlave chain walks.
- [`../../modules/scripting.md`](../../modules/scripting.md) — `/script` and the privileged-spawn flag.
- [`ScriptingCommands.bb`'s `BVM_OUTPUT`](../../../src/Modules/ScriptingCommands.bb#L2785) — the only S→C `Chr$(250)` (custom-RGB) emitter.
