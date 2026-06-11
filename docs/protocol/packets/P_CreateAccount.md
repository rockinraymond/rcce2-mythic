# P_CreateAccount

**Direction:** C -> S (request), S -> C (single-byte result reply)
**Numeric ID:** 1 ([Packets.bb:2](../../../src/Modules/Packets.bb#L2))
**Client send site:** [MainMenu.bb:1204](../../../src/Modules/MainMenu.bb#L1204) (the "New account" button handler; reply read at [MainMenu.bb:1216-1218](../../../src/Modules/MainMenu.bb#L1216))
**Server handler:** [ServerNet.bb:2344](../../../src/Modules/ServerNet.bb#L2344) (`Case P_CreateAccount`)

## Purpose

Pre-authentication account registration. A peer that has connected but not logged in submits a desired username, an MD5 of the desired password, and an (obfuscated) email address. The server validates the character set, rejects duplicates, and on success appends a new `Account` record to `Accounts.dat` via [`AddAccount`](../../../src/Modules/AccountsServer.bb#L193).

This is one of three C->S pre-auth packets in the account cluster ([`P_VerifyAccount`](P_VerifyAccount.md), [`P_ChangePassword`](P_ChangePassword.md)). Any connected peer can send any bytes; the handler runs **before** any identity is established.

The whole handler is gated by `If AllowAccountCreation = True` ([ServerNet.bb:2345](../../../src/Modules/ServerNet.bb#L2345)) — a server-config byte read from the server settings file at [Server.bb:262](../../../src/Modules/ServerNet.bb#L262) (declared [Server.bb:152](../../../src/Modules/ServerNet.bb#L152)). When account creation is disabled the handler **silently no-ops**: no reply is sent at all (see Anti-cheat surface).

## Field layout

The password is sent as the 32-char lowercase-hex MD5 of the typed password (`MD5Pass$ = MD5$(Pass$)`), not the plaintext. The email is obfuscated client-side with `Encrypt$(Email$, -1)` (a reversible Caesar-shift-plus-reverse, see [Client.bb:1034](../../../src/Client.bb#L1034)) and de-obfuscated server-side with `Encrypt$(..., 1)`.

Request body, built at [MainMenu.bb:1200-1202](../../../src/Modules/MainMenu.bb#L1200):

| # | Field | Width | Sender write (MainMenu.bb) | Receiver read (ServerNet.bb) |
|---|---|---|---|---|
| 1 | Username length | 1 byte | `RCE_StrFromInt$(Len(Name$), 1)` | `RCE_IntFromStr(Left$(M\MessageData$, 1))` -> `UsernameLen` [:2346](../../../src/Modules/ServerNet.bb#L2346) |
| 2 | Username | `UsernameLen` bytes | `Name$` (concat) | `Mid$(M\MessageData$, 2, UsernameLen)` [:2347](../../../src/Modules/ServerNet.bb#L2347) |
| 3 | Password (MD5 hex) length | 1 byte | `RCE_StrFromInt$(Len(MD5Pass$), 1)` | `RCE_IntFromStr(Mid$(M\MessageData$, Offset, 1))` where `Offset = 2 + UsernameLen` [:2359-2360](../../../src/Modules/ServerNet.bb#L2359) |
| 4 | Password (MD5 hex) | `PwdLen` bytes | `MD5Pass$` (concat) | `Mid$(M\MessageData$, Offset + 1, PwdLen)` [:2361](../../../src/Modules/ServerNet.bb#L2361) |
| 5 | Email length | 1 byte | `RCE_StrFromInt$(Len(Email$), 1)` | `RCE_IntFromStr(Mid$(M\MessageData$, Offset, 1))` where `Offset = Offset + 1 + PwdLen` [:2362-2363](../../../src/Modules/ServerNet.bb#L2362) |
| 6 | Email (Caesar-obfuscated) | `EmailLen` bytes | `Encrypt$(Email$, -1)` (concat) | `Encrypt$(Mid$(M\MessageData$, Offset + 1, EmailLen), 1)` [:2364](../../../src/Modules/ServerNet.bb#L2364) |

All length prefixes are **1 byte** here (not the 4-byte file-string convention) — sender and receiver widths match for every field. Note field 5's length prefix is written by the client as `RCE_StrFromInt$(Len(Email$), 1)` over the **plaintext** length, and `Encrypt$` is a 1:1 byte transform (it does not change length), so the prefix correctly counts the obfuscated bytes.

### Reply

| Reply byte | Meaning | Server emit | Client action |
|---|---|---|---|
| `"Y"` | Account created | [ServerNet.bb:2388](../../../src/Modules/ServerNet.bb#L2388) | `Result = True` -> `LS_NewAccountCreated` message ([MainMenu.bb:1240](../../../src/Modules/MainMenu.bb#L1240)) |
| `"N"` | Duplicate username **or** failed validation | [ServerNet.bb:2390](../../../src/Modules/ServerNet.bb#L2390) / [:2393](../../../src/Modules/ServerNet.bb#L2393) | any non-`"Y"` -> `Result = False` -> `LS_UsernameAlreadyExists` message ([MainMenu.bb:1238](../../../src/Modules/MainMenu.bb#L1238)) |
| *(no reply)* | `AllowAccountCreation = False` | handler body skipped | client waits in its receive loop until disconnect / Escape |

## Validation requirements (server-side)

All server-side, since the packet is attacker-controlled. In sequence:

1. **`AllowAccountCreation = True`** ([:2345](../../../src/Modules/ServerNet.bb#L2345)) — config gate. False -> no-op, no reply.
2. **Duplicate-username check** ([:2349-2356](../../../src/Modules/ServerNet.bb#L2349)) — `For A.Account = Each Account : If Upper$(A\User$) = Upper$(Username$) Then Exists = True`. Case-insensitive via `Upper$`. On duplicate, reply `"N"` and stop ([:2393](../../../src/Modules/ServerNet.bb#L2393)).
3. **Username character set** ([:2367-2370](../../../src/Modules/ServerNet.bb#L2367)) — each byte must be `0-9` (48-57), `A-Z` (65-90), `a-z` (97-122), `_` (95), **or `>= 192`** (high/extended bytes pass). Any other byte sets `Valid = False`.
4. **Password (MD5 hex) character set** ([:2372-2375](../../../src/Modules/ServerNet.bb#L2372)) — same alnum set plus `.` (46), `_` (95), or `>= 192`. (The client always sends 32 lowercase hex chars, which trivially pass; the check guards a hand-rolled packet.)
5. **Email character set** ([:2377-2380](../../../src/Modules/ServerNet.bb#L2377)) — validated **after** `Encrypt$(..., 1)` de-obfuscation, so the check runs on the plaintext email. Allowed: alnum, `@` (64), `*` (42), `+` (43), `-` (45), `.` (46), `=` (61), `_` (95), or `>= 192`.
6. **Length caps** ([:2381](../../../src/Modules/ServerNet.bb#L2381)) — `Len(Username$) > 50 Or Len(Password$) > 50 Or Len(Email$) > 200` -> `Valid = False`.
7. On `Valid = True`, [`AddAccount(Username$, Password$, Email$)`](../../../src/Modules/AccountsServer.bb#L193) creates the record (storing the password as a salted SHA-256 v1 hash via [`HashPassword$`](../../../src/Modules/PasswordHash.bb#L209)) and appends to `Accounts.dat`; reply `"Y"`. Otherwise reply `"N"`.

### Gaps in the validation (verified against source)

These are **not** defended and are documented here honestly rather than implied:

- **No minimum-length / non-empty check.** A zero-length username passes step 3 (the `For i = 1 To Len(Username$)` loop never runs, so `Valid` stays `True`). The client enforces `Len(Name$) >= 2` ([MainMenu.bb:1195](../../../src/Modules/MainMenu.bb#L1195)) and `Len(Pass$) >= 2` ([:1196](../../../src/Modules/MainMenu.bb#L1196)), but a hand-rolled packet bypasses that — the server will create an empty-username / empty-password account.
- **No `LoginAttemptOk` throttle.** Unlike [`P_VerifyAccount`](P_VerifyAccount.md), `P_StartGame`, `P_FetchCharacter`, `P_CreateCharacter`, and `P_DeleteCharacter` (all throttled by PR [#266](https://github.com/RydeTec/rcce2/pull/266) / [#268](https://github.com/RydeTec/rcce2/pull/268)), `P_CreateAccount` has no per-source rate limit. See Anti-cheat surface.

## Anti-cheat / abuse surface

`P_CreateAccount` runs before authentication; any connected peer can send it repeatedly.

- **Account-creation flooding (undefended).** There is no throttle and no minimum-length check, so a peer can create accounts at line rate (subject only to the `AllowAccountCreation` config gate). Each accepted account is appended to `Accounts.dat` ([AccountsServer.bb:207-213](../../../src/Modules/AccountsServer.bb#L207)) and added to an in-memory list, so a flood grows the file and the `Account` list unboundedly. The practical mitigation today is operational: run with `AllowAccountCreation = False` and provision accounts out-of-band, or front the server with a network-level rate limiter. This is the most notable gap relative to the sibling auth handlers, which were all given the `LoginAttemptOk` throttle.
- **Username squatting / enumeration via duplicate check.** The `"N"` reply collapses *both* "duplicate username" and "failed validation" into one code, so the reply does **not** by itself leak whether a username already exists vs. was malformed. (Contrast the historical `P_VerifyAccount` enumeration oracle, which *was* exploitable — see [`P_VerifyAccount`](P_VerifyAccount.md).) However, a probe with a known-valid character set still discriminates "taken" (`"N"`) from "created" (`"Y"`), which is an inherent property of any open-registration endpoint.
- **Password is MD5-over-the-wire.** The wire carries `MD5$(plaintext)`; the at-rest copy is salted SHA-256 of that MD5 ([PasswordHash.bb storage notes](../../../src/Modules/PasswordHash.bb#L19)). A wire sniffer who captures the registration packet learns the MD5, which is replayable against this server forever — true of the entire account cluster. Closing that requires TLS or challenge/response; see the module header at [PasswordHash.bb:14-17](../../../src/Modules/PasswordHash.bb#L14).
- **Misleading client message.** The client renders **any** non-`"Y"` reply as `LS_UsernameAlreadyExists` ([MainMenu.bb:1238](../../../src/Modules/MainMenu.bb#L1238)), so a registration rejected for an invalid character or an over-length field is shown to a legitimate user as "username already exists." This is a UX defect, not a security one.

## Historical bugs / PR references

| PR / commit | Relevance |
|---|---|
| PR [#118](https://github.com/RydeTec/rcce2/pull/118) (`88aaf393`) | Server-side password-hash migration. `AddAccount` now stores `HashPassword$(Pass$)` (salted SHA-256 v1) instead of the raw client MD5, so theft of `Accounts.dat` no longer yields working wire credentials. The wire format (client sends MD5) is unchanged. |
| PR [#266](https://github.com/RydeTec/rcce2/pull/266) / [#268](https://github.com/RydeTec/rcce2/pull/268) | Introduced and extended the `LoginAttemptOk` throttle across the auth handlers — **but not** `P_CreateAccount`. The absence here is current behavior, not an oversight that was later patched. |

No dedicated regression test exists for `P_CreateAccount` (contrast [`AccountEnumerationTest.bb`](../../../src/Tests/Modules/AccountEnumerationTest.bb) for `P_VerifyAccount`). The validation logic is exercised only end-to-end.

## Related packets

- [`P_VerifyAccount`](P_VerifyAccount.md) — the login counterpart; reuses the same `1B-len + name + 1B-len + MD5` username/password framing.
- [`P_ChangePassword`](P_ChangePassword.md) — password rotation for an existing account; same framing plus a second password block.

## See also

- [`../encoding.md`](../encoding.md) — `RCE_StrFromInt$` / `RCE_IntFromStr`, 1-byte length-prefix convention.
- [`../handler-conventions.md`](../handler-conventions.md) — bounds-then-deref, soft-fail, and pre-auth handler discipline.
- [`AccountsServer.bb`'s `AddAccount`](../../../src/Modules/AccountsServer.bb#L193) — the record-creation + atomic-append path.
- [`PasswordHash.bb`](../../../src/Modules/PasswordHash.bb) — salted SHA-256 v1 storage format and `HashPassword$`.
