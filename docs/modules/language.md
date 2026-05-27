<!-- body { color:black background-color:white } a:link{ color:#0070FF } a:visited{ color:#0070FF } --> RealmCrafter: Community Edition Documentation

**Language.bb**

The i18n / localization layer. Defines the 227-entry `LS_*` string-ID constant family (used everywhere the engine emits a human-readable string), the `LanguageString$()` registry that backs them, and the `LoadLanguage` / `RestoreLanguage` file I/O for swapping locale `.txt` files at boot.

Every player-facing string in the client should resolve through `LanguageString$(LS_*)` — never hard-coded. The constants live in this file; the runtime values live in `Data\Game Data\Language.txt` (or whatever locale the project ships).

## Conceptual overview

### The LS_* constant family

```basic
Const MaxLanguageString = 226       ; inclusive — actual count is 227
Const LS_ConnectingToServer = 0
Const LS_FileProgress       = 1
...
Const LS_AccountAlreadyConnected = 226
Dim LanguageString$(MaxLanguageString)
```

227 entries indexed `0..226` (Blitz3D `Dim X(N)` allocates `N+1` slots — see CLAUDE.md → "Gotchas"). The constants are flat — no enum / namespace — so to add a new string you must:

1. Bump `Const MaxLanguageString` from 226 to 227.
2. Add `Const LS_YourNewString = 227`.
3. Append a new line at the end of `Data\Game Data\Language.txt` with the localized value.
4. Update the slash-command range guard in `LoadLanguage` (see below) if the new constant lives between 190 and 219.

Missing any one of these silently produces an empty string at the new slot (no error, just blank UI text).

### The four broad string categories

The constants are grouped by purpose; the file's comments mark transitions:

| Range | Category | Examples | Touched by |
|---|---|---|---|
| **0–189** | Engine / UI strings | `LS_ConnectingToServer`, `LS_InvalidPassword`, `LS_QuestLogUpdate`, `LS_YouHit`, `LS_NoInventorySpace`, item-type names, control bindings, attribute labels | [`MainMenu.bb`](mainmenu.md), [`Interface.bb`](interface.md), [`ClientNet.bb`](clientnet.md), [`ClientCombat.bb`](clientcombat.md) |
| **190–219** | Slash-command names (UPPERCASE) | `LS_SCKick = 190` → `"KICK"`, `LS_SCYell = 204` → `"YELL"`, `LS_SCWarp = 212` → `"WARP"` | [`ServerNet.bb`](servernet.md) chat-command dispatch; matched against incoming `/<command>` text |
| **220–225** | Quit / pause dialog | `LS_QuitToContinue` → `"Back to Game"`, `LS_QuitRequestText` → `"Do you want to leave immediately or wait for 10 seconds?"` | [`MainMenu.bb`](mainmenu.md) quit overlay |
| **226** | Late addition | `LS_AccountAlreadyConnected` → `"This account is already connected"` | Auth-handler in [`ServerNet.bb`](servernet.md) |

The slash-command range is special-cased: `LoadLanguage` upper-cases everything in `[LS_SKick..LS_SSeason]` (190..219) on load so the runtime string-compare against `/<command>` is case-insensitive without needing per-compare normalization. The audit-comment at line 301 documents this. If you add new slash-command constants outside that range, the upper-casing won't apply and `/<command>` matching will be case-sensitive.

> **Constant-name typo**: the source uses `LS_SKick` / `LS_SSeason` in the `LoadLanguage` range check but the actual constant names are `LS_SCKick` / `LS_SCSeason`. These resolve to the same numeric values (190 / 219) — undeclared identifiers in non-Strict files default to 0, so `LS_SKick = 0` and `LS_SSeason = 0` would make the range `[0..0]`, which silently *under-applies* the upper-case rule. This is a real source bug if the file is not Strict — verify against `If ID >= LS_SKick And ID <= LS_SSeason` at [`Language.bb:301`](../../src/Modules/Language.bb#L301). (Confirm or file as follow-up.)

### File format

`Data\Game Data\Language.txt` is one localized string per line, in the same order as the `LS_*` constants (line N maps to constant N). `LoadLanguage` ignores:

- Blank lines (skipped entirely; do **not** count against the line/ID counter).
- Full-line comments (lines starting with `;`).
- Trailing comments (everything from the first `;` to end-of-line is stripped).

So a fully-commented row counts as **no row** — it does not consume an `ID`. This means re-ordering the file with comment markers as section dividers is safe, but accidentally commenting out a real entry will shift every subsequent string by one slot. **Never reorder `Language.txt` without bumping the matching `LS_*` constant definitions.**

### Restoration / hot-reload

`RestoreLanguage(Filename$)` writes the current in-memory `LanguageString$()` array out to a file — used by tooling to dump the active locale back to disk after edits. It first reloads `Data\Game Data\Language.txt` so a partial in-memory state isn't persisted. The output is plain `WriteLine` per slot; comments are not preserved (the input-side comment stripping is one-way).

There is no hot-reload of `Data\Game Data\Language.txt` at runtime — strings are read once at boot in `MainMenu.bb`. To test a localized string change, restart the client.

## Conventions for new code touching this module

- **Add the constant, bump `MaxLanguageString`, append to `Language.txt`** — all three are required. Missing any one is a silent failure.
- **Use `LanguageString$(LS_FooBar)` at the call site**, never the integer literal. The integer is a stable wire-protocol-style number that **could** be re-numbered between releases (it hasn't been), but the constant name is the contract.
- **Don't re-order `Language.txt`** without re-ordering the `LS_*` constants in lockstep. Line N ↔ constant N is the file format.
- **Slash-command additions go in `[190..219]`** if they need the auto-upper-casing; otherwise place after `LS_AccountAlreadyConnected` and add a matching `LoadLanguage` range guard. The current `LS_SCKick..LS_SCSeason` range covers `KICK / UNIGNORE / IGNORE / NETDUMP / PET / LEAVE / ACCEPT / INVITE / XP / GOLD / SETATTRIBUTE / SETATTRIBUTEMAX / SCRIPT / ME / YELL / GM / G / P / PM / TRADE / ALLPLAYERS / PLAYERS / WARP / WARPOTHER / ABILITY / GIVE / WEATHER / TIME / DATE / SEASON`.
- **The defaults block (lines 241-279)** seeds slash-command constants + the quit-dialog strings. These are fallback values used when `Language.txt` can't be loaded; new entries that lack a corresponding default block line will be empty if the file is missing.

## Related modules

- [`MainMenu.bb`](mainmenu.md) — calls `LoadLanguage("Data\Game Data\Language.txt")` at boot. The biggest consumer of `LanguageString$(LS_*)` (login screen, character-create, options pages).
- [`Interface.bb`](interface.md) — 2D in-game UI; consumes `LS_Weapon` / `LS_Armour` / item-type names, `LS_YouHit` damage text, `LS_QuestLogUpdate`, etc.
- [`ServerNet.bb`](servernet.md) — chat-command dispatch matches `/<word>` against the slash-command range (190..219).
- [`ClientCombat.bb`](clientcombat.md) — combat-log strings (`LS_YouHit`, `LS_For`, `LS_DamageWow`, `LS_HitsYou`, `LS_AttacksYouMisses`, `LS_YouAttack`, `LS_AndMiss`, `LS_CriticalDamage`).

## See also

- [`P_SpellUpdate` detail](../protocol/packets/P_SpellUpdate.md) — UX strings around spell-cast gating (`LS_RaceOnly`, `LS_ClassOnly`, `LS_AbilityNotRecharged`).
- CLAUDE.md → "Gotchas" → "Blitz3D array semantics" — `Dim X(N)` is `N+1` inclusive.

* * *

This module's source is short enough that a function-by-function reference adds little — read [`src/Modules/Language.bb`](../../src/Modules/Language.bb) directly. The two public functions are:

- **`LoadLanguage(Filename$)`** — open `Filename$`, parse line-by-line (skipping `;` comments, stripping trailing comments, ignoring blanks), assign to `LanguageString$(0..MaxLanguageString)`. Upper-cases slash-command range. Returns `True` on success, `False` if the file couldn't be opened. **`RuntimeError`s if the file has more than `MaxLanguageString + 1` non-blank, non-comment lines** — bumping `MaxLanguageString` is mandatory when adding entries.
- **`RestoreLanguage(Filename$)`** — re-load production `Language.txt`, then dump every `LanguageString$(i)` to `Filename$` one per line. Returns `True` / `False` for write success.
