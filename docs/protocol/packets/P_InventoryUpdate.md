# P_InventoryUpdate

**Direction:** Both (C→S and S→C, with different sub-codes per direction)
**Numeric ID:** 15
**Server handler:** [ServerNet.bb:1573](../../../src/Modules/ServerNet.bb#L1573)
**Client handler:** [ClientNet.bb:1271](../../../src/Modules/ClientNet.bb#L1271)

## Purpose

Carries every inventory mutation between client and server: drop / pickup of world items, slot-to-slot moves, stack splits, give-item dialogs, and the client-side visual updates that follow a server-side change. The richest sub-code structure of any packet in the protocol (12 sub-codes total across the two directions); each sub-code has a distinct field layout.

The dispatcher's first byte is the sub-code; everything after depends on it.

## Sub-codes

### Client → server

| Sub-code | Purpose | Field layout |
|---|---|---|
| `"P"` | **Pickup** a dropped item from the floor | `1B sub + 4B DroppedHandle + 1B SlotI` |
| `"D"` | **Drop** an item from inventory to the floor | `1B sub + 1B Slot + 2B Amount` |
| `"G"` | **Give-item reply**: Y = take, N = decline | `1B sub + 1B ("Y"/"N") + 4B ItemInstance handle + 1B SlotI (if Y)` |
| `"S"` | **Swap** items between two slots | `1B sub + 2B TargetRuntimeID + 1B SlotA + 1B SlotB + 2B Amount` |
| `"A"` | **Add to stack** (merge into another slot of the same item) | `1B sub + 2B TargetRuntimeID + 1B SlotFrom + 1B SlotTo + 2B Amount` |

### Server → client

| Sub-code | Purpose | Field layout |
|---|---|---|
| `"H"` | Item **health** changed (durability decrement) | `1B sub + 1B SlotI + 1B Amount` |
| `"T"` | Item **taken** from my inventory | `1B sub + 1B SlotI + 2B Amount` |
| `"R"` | **Received** a dropped item I picked up | `1B sub + 4B ServerHandle + 1B SlotI` |
| `"P"` | A dropped item I see has been **picked up** by another player | `1B sub + 4B Handle(DroppedItem)` |
| `"D"` | A new item was **dropped** to the floor near me | `1B sub + 2B Amount + 4B X + 4B Y + 4B Z + 4B DroppedHandle + ItemInstanceToString$` |
| `"O"` | **Offered** an item via give-item dialog | (server-controlled item-offer flow) |
| `"G"` | Generic **gold/inventory** update tail | (item assignment / quest reward path) |

## Validation requirements

Per the [handler conventions](../handler-conventions.md):

### Same-area gate on "P" (pickup)

ServerNet.bb:1589 — `If D\ServerHandle = AI\ServerArea` plus a distance check `PickupDist# <= InteractDist + 50.0`. Without these, anyone holding (or guessing — handles are sequential 4-byte integers) the `DroppedItem` handle could pick up an item from another area or across the map. The 50-unit slack over `InteractDist` lets players legitimately grab items at the edge of their interaction radius.

### Slot index bounds on every sub-code

`SlotI` and the various `Slot*` parameters come straight off the wire as 1-byte or 2-byte ints. Every site does:

```basic
If SlotI < 0 Or SlotI > Slots_Inventory Then SlotI = -1
```

Then the downstream code guards on `SlotI >= 0`. Without this, array reads against `Items[]` and `Amounts[]` walk past the `Inventory` record into the adjacent `ActorInstance` fields — was a memory-corruption surface before the bounds-check sweep landed.

### Amount sign + range on InventoryAdd

PR [#276](https://github.com/RydeTec/rcce2/pull/276) discovered that the wire-level check
```basic
If (AI = AIFrom Or IsPet) And (Amount = 0 Or Amount <= AI\Inventory\Amounts[SlotA])
```
PASSED for any negative `Amount` (negative ≤ non-negative is always true). The downstream `Amounts[SlotTo] += Amount` / `Amounts[SlotFrom] -= Amount` arithmetic then inflated the source slot and deflated the destination — an unbounded item-duplication path. The fix added an internal guard in `Inventories.bb`'s `InventoryAdd`:

```basic
If Amount < 1 Or Amount > A\Inventory\Amounts[SlotFrom] Then Return False
```

The matching guard in `InventorySwap` (line 152) was already present — the absence in `InventoryAdd` was a sibling-protection-asymmetry bug. See [`handler-conventions.md#sibling-protection-asymmetry`](../handler-conventions.md).

### Item-handle assignment match on "G"

ServerNet.bb:1681 — `If II\Assignment > 0 And II\AssignTo = AI`. Without the `AssignTo` check, anyone holding (or guessing) a 4-byte ItemInstance handle could claim items intended for another player. The `P_OpenTrading` buy path uses the same `Assignment` / `AssignTo` pair.

### Pet/Slave validation on swap/add

ServerNet.bb:1722-1733 — when `RuntimeID` doesn't equal the sender, the handler walks the sender's `For Slave.ActorInstance = Each ActorInstance / If Slave\Leader = AIFrom` to verify the target is one of the sender's pets. Without this gate, anyone could rearrange another player's inventory.

### Float sanitisation on "D" drop position

ServerNet.bb:1654-1656 — the dropped-item position uses `ClampWorldCoord#(AI\X#)` etc. before being written to `DroppedItem` and broadcast. `AI\X#/Y#/Z#` can carry a NaN/Inf from an upstream unvalidated packet; a NaN dropped-item position poisons spatial code on every receiver.

## Historical bugs / PR references

| PR | Fixed |
|---|---|
| [#242](https://github.com/RydeTec/rcce2/pull/242) | Tightened SlotIndex bound on P_ItemScript / P_EatItem (sibling pattern) |
| [#276](https://github.com/RydeTec/rcce2/pull/276) | Negative-Amount InventoryAdd duplication; range gates on P_Examine / P_Trade / P_ItemScript |
| [#283](https://github.com/RydeTec/rcce2/pull/283) | "P" pickup and "D" drop broadcasts now walk FirstOnlinePlayer / FirstInZone chains instead of `Each ActorInstance` |

## Related packets

- [`P_ItemScript`](../index.md) — script-spawning use of an item from inventory
- [`P_EatItem`](../index.md) — consumable item use
- [`P_OpenTrading`](../index.md) — player-to-player trade; shares the `II\Assignment` / `II\AssignTo` mechanism with the "G" reply path

## See also

- [`../encoding.md`](../encoding.md) — `RCE_StrFromInt$`, `ClampWorldCoord#`, length conventions
- [`../handler-conventions.md`](../handler-conventions.md) — bounds-check, sibling-protection asymmetry, soft-fail discipline
- [`../../modules/inventories.md`](../../modules/inventories.md) — `InventoryDrop` / `InventoryAdd` / `InventorySwap` primitives this packet drives
- [`../../modules/items.md`](../../modules/items.md) — `Item`, `ItemInstance`, `DroppedItem` types
