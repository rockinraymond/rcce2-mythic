Strict
EnableGC

; Regression pins for the equip-slot / slot-exclusivity rules in
; src/Modules/Inventories.bb: ActorHasSlot (race/class exclusivity fork +
; per-slot disabled-flag gating), SlotsMatch (item-type -> slot-index
; mapping), and the InventorySwap / InventoryAdd / InventoryDrop callers
; (equip round-trip, stack-into-equip-slot rejection, partial-stack split,
; merge ceiling, drop drain).
;
; These are PIN-CURRENT-BEHAVIOR tests: they assert what the code actually
; computes today, including the surprising forks. Anything that looks like
; a latent bug is pinned as-is and marked with a `; FLAG-FOR-HUMAN:`
; comment at the assertion. Current flags:
;   1. ActorHasSlot's exclusivity fork gates on `SlotI < Slot_Backpack`
;      (the 1-based slot NAME constant, 11) instead of SlotI_Backpack (the
;      slot INDEX, 14), so indices 11-13 (Ring4, Amulet1, Amulet2) skip
;      the ExclusiveRace$/ExclusiveClass$ checks entirely.
;   2. A matching ExclusiveRace$ returns True immediately, skipping both
;      the ExclusiveClass$ check and the disabled-slot flag check.
;
; Inventories.bb is pulled in for real (this is the point -- the earlier
; InventoryStackClampTest replicated logic because it avoided the include;
; including Items.bb first supplies ItemInstance, so the include works fine
; with a few stubs, same pattern as ItemsTest.bb).

; --- External type stubs ----------------------------------------------------
; Items.bb references Attributes / ActorInstance (defined in Actors.bb);
; Inventories.bb additionally needs Actor (Race$/Class$/InventorySlots) and
; an ActorInstance shaped with Inventory/Actor/RuntimeID. Forward field
; references to the Inventory type (defined later, inside Inventories.bb)
; are fine -- the real build does the same (Items.bb's AssignTo.ActorInstance
; precedes Actors.bb in Server.bb's include order).
Type Attributes
	Field Value[39]
	Field Maximum[39]
	Field My_ID
End Type

Type Actor
	Field Race$
	Field Class$
	Field InventorySlots
End Type

Type ActorInstance
	Field Account               ; referenced by Items.bb
	Field Actor.Actor           ; referenced by Inventories.bb
	Field Inventory.Inventory
	Field RuntimeID
End Type

; --- RCE wire-format helpers ------------------------------------------------
; Verbatim from RCEnet.bb with a private Bank (same as ItemsTest.bb).
Global InvTest_ConvertBank.BBBank = CreateBank(8)

Function RCE_IntFromStr(Dat$)
	PokeInt InvTest_ConvertBank, 0, 0
	Local i
	For i = 1 To Len(Dat$)
		PokeByte InvTest_ConvertBank, i - 1, Asc(Mid$(Dat$, i, 1))
	Next
	Return PeekInt(InvTest_ConvertBank, 0)
End Function

Function RCE_StrFromInt$(Num, Length = 4)
	PokeInt InvTest_ConvertBank, 0, Num
	Local Dat$ = ""
	Local i
	For i = Length - 1 To 0 Step -1
		Dat$ = Chr$(PeekByte(InvTest_ConvertBank, i)) + Dat$
	Next
	Return Dat$
End Function

; --- Network stubs ----------------------------------------------------------
; Every Inventory* call below passes TellServer = False, so the send path is
; never taken; the stub only needs to satisfy the compiler. Signature mirrors
; RCEnet.bb's RCE_Send (param names changed to avoid Strict shadowing of the
; Connection global).
Global Connection = 0
Global PeerToHost = 0
Const P_InventoryUpdate = 0

Function RCE_Send(Conn, Destination, MessageType, MessageData$, ReliableFlag = 0, PlayerFrom = 0, DoNotUse = 0, ConfirmID = -1)
End Function

; --- GetFlag (Actors.bb) ----------------------------------------------------
; Replicated verbatim -- one expression; pulling Actors.bb in would drag the
; world graph.
Function GetFlag(TheInt, Flag)
	Return (TheInt Shr Flag) And 1
End Function

; --- Logging stub -----------------------------------------------------------
Global MainLog = 0

Function WriteLog(LogID%, Message$, Timestamp% = True, Datestamp% = False)
End Function

; --- SafeWrite stubs (Items.bb save path, not exercised) ---------------------
Function SafeWriteOpen$(FinalPath$)
	Return FinalPath$
End Function

Function SafeWriteCommit%(TempPath$, FinalPath$, F)
	Return True
End Function

; --- Language helper stub (Items.bb GetItemType$/GetWeaponType$) -------------
Function LanguageString$(key$)
	Return key
End Function

; --- ReadBoundedString$ stub (Items.bb load path, not exercised) -------------
Function ReadBoundedString$(F, MaxLen)
	Return ""
End Function

Include "Modules\Items.bb"
Include "Modules\Inventories.bb"

; --- Test helpers -----------------------------------------------------------

; Bit mask with every inventory-slot flag enabled (bits 0..10:
; Weapon, Shield, Hat, Chest, Hand, Belt, Legs, Feet, Ring, Amulet, Backpack).
Const MaskAll = 2047            ; $7FF

; Make an actor instance with an inventory and the given slot-flag mask.
Function MakeActor.ActorInstance(slotsMask, race$ = "", cls$ = "")
	Local A.ActorInstance = New ActorInstance()
	A\Actor = New Actor()
	A\Actor\InventorySlots = slotsMask
	A\Actor\Race$ = race
	A\Actor\Class$ = cls
	A\Inventory = New Inventory()
	Return A
End Function

; Register an Item template with the given type/slot-type.
Function SeedTypedItem.Item(name$, itemType, slotType)
	Local It.Item = CreateItem()
	It\Name$ = name
	It\ItemType = itemType
	It\SlotType = slotType
	Return It
End Function

; Place a fresh instance of an Item into an inventory slot.
Function GiveItem.ItemInstance(A.ActorInstance, slot, It.Item, amount)
	Local Inst.ItemInstance = CreateItemInstance(It)
	A\Inventory\Items[slot] = Inst
	A\Inventory\Amounts[slot] = amount
	Return Inst
End Function

; Reset all module + stub state between tests.
Function ClearAll()
	Delete Each ActorInstance
	Delete Each Actor
	Delete Each Inventory
	Delete Each ItemInstance
	Delete Each Item
End Function

; ---------------------------------------------------------------------------
; ActorHasSlot: disabled-slot flag gating
; ---------------------------------------------------------------------------

; With no exclusivity set, ActorHasSlot is a straight bit test against
; Actor\InventorySlots: bit 0 = weapon ... bit 8 = all four ring indices,
; bit 9 = both amulet indices, bit 10 = every backpack index (14..45).
Test testActorHasSlotGatesOnSlotFlags()
	ClearAll()
	Local plain.Item = SeedTypedItem("Plain", I_Weapon, 0)
	Local A.ActorInstance = MakeActor(0)

	; All flags off: every slot index denied, including backpack.
	Assert(ActorHasSlot(A\Actor, SlotI_Weapon, plain) = False)
	Assert(ActorHasSlot(A\Actor, SlotI_Feet, plain) = False)
	Assert(ActorHasSlot(A\Actor, SlotI_Ring1, plain) = False)
	Assert(ActorHasSlot(A\Actor, SlotI_Amulet2, plain) = False)
	Assert(ActorHasSlot(A\Actor, SlotI_Backpack, plain) = False)
	Assert(ActorHasSlot(A\Actor, Slots_Inventory, plain) = False)

	; Weapon bit only: weapon index allowed, neighbours still denied.
	A\Actor\InventorySlots = 1
	Assert(ActorHasSlot(A\Actor, SlotI_Weapon, plain) = True)
	Assert(ActorHasSlot(A\Actor, SlotI_Shield, plain) = False)

	; Ring bit (8) covers all four ring indices; amulet bit (9) both amulets;
	; backpack bit (10) the whole 14..45 range.
	A\Actor\InventorySlots = (1 Shl 8)
	Assert(ActorHasSlot(A\Actor, SlotI_Ring1, plain) = True)
	Assert(ActorHasSlot(A\Actor, SlotI_Ring4, plain) = True)
	Assert(ActorHasSlot(A\Actor, SlotI_Amulet1, plain) = False)
	A\Actor\InventorySlots = (1 Shl 9)
	Assert(ActorHasSlot(A\Actor, SlotI_Amulet1, plain) = True)
	Assert(ActorHasSlot(A\Actor, SlotI_Amulet2, plain) = True)
	Assert(ActorHasSlot(A\Actor, SlotI_Ring4, plain) = False)
	A\Actor\InventorySlots = (1 Shl 10)
	Assert(ActorHasSlot(A\Actor, SlotI_Backpack, plain) = True)
	Assert(ActorHasSlot(A\Actor, Slots_Inventory, plain) = True)
	Assert(ActorHasSlot(A\Actor, SlotI_Weapon, plain) = False)

	ClearAll()
End Test

; ---------------------------------------------------------------------------
; ActorHasSlot: race exclusivity fork (Inventories.bb ~line 230)
; ---------------------------------------------------------------------------

; A matching ExclusiveRace$ returns True IMMEDIATELY -- even when the slot's
; flag is disabled (the code comments this as intentional: "Allow even
; disabled equipment slots to be used if the item is exclusive to this
; race"). Comparison is case-insensitive.
Test testActorHasSlotRaceMatchOverridesDisabledSlot()
	ClearAll()
	Local elfBlade.Item = SeedTypedItem("Elf Blade", I_Weapon, 0)
	elfBlade\ExclusiveRace$ = "Elf"
	Local A.ActorInstance = MakeActor(0, "elf")   ; all slots DISABLED

	Assert(ActorHasSlot(A\Actor, SlotI_Weapon, elfBlade) = True)
	Assert(ActorHasSlot(A\Actor, SlotI_Chest, elfBlade) = True)
	Assert(ActorHasSlot(A\Actor, SlotI_Ring3, elfBlade) = True)

	; The override only applies to the exclusivity-checked index range
	; (SlotI < Slot_Backpack = 11). A backpack index still falls through to
	; the (disabled) backpack flag.
	Assert(ActorHasSlot(A\Actor, SlotI_Backpack, elfBlade) = False)

	ClearAll()
End Test

; The wrong race is denied outright on the exclusivity-checked indices, no
; matter what the slot flags say.
Test testActorHasSlotWrongRaceDenied()
	ClearAll()
	Local orcAxe.Item = SeedTypedItem("Orc Axe", I_Weapon, 0)
	orcAxe\ExclusiveRace$ = "Orc"
	Local A.ActorInstance = MakeActor(MaskAll, "Elf")   ; all slots ENABLED

	Assert(ActorHasSlot(A\Actor, SlotI_Weapon, orcAxe) = False)
	Assert(ActorHasSlot(A\Actor, SlotI_Feet, orcAxe) = False)
	Assert(ActorHasSlot(A\Actor, SlotI_Ring1, orcAxe) = False)
	Assert(ActorHasSlot(A\Actor, SlotI_Ring3, orcAxe) = False)

	; FLAG-FOR-HUMAN: the exclusivity fork gates on `SlotI < Slot_Backpack`
	; -- Slot_Backpack is the 1-based slot NAME constant (11), not the slot
	; INDEX SlotI_Backpack (14). Slot indices 11..13 (Ring4, Amulet1,
	; Amulet2) therefore SKIP the race/class exclusivity check entirely: a
	; wrong-race ring is denied in Ring1-3 but ALLOWED in Ring4, and a
	; wrong-race amulet is allowed in both amulet slots (flag permitting).
	Assert(ActorHasSlot(A\Actor, SlotI_Ring4, orcAxe) = True)
	Assert(ActorHasSlot(A\Actor, SlotI_Amulet1, orcAxe) = True)
	Assert(ActorHasSlot(A\Actor, SlotI_Amulet2, orcAxe) = True)

	; Backpack indices never run the exclusivity check either -- a
	; wrong-race item can always sit in the backpack (flag permitting).
	Assert(ActorHasSlot(A\Actor, SlotI_Backpack, orcAxe) = True)

	ClearAll()
End Test

; ---------------------------------------------------------------------------
; ActorHasSlot: class exclusivity fork (Inventories.bb ~line 241)
; ---------------------------------------------------------------------------

Test testActorHasSlotClassExclusivity()
	ClearAll()
	Local staff.Item = SeedTypedItem("Mage Staff", I_Weapon, 0)
	staff\ExclusiveClass$ = "Mage"

	; Wrong class: denied on exclusivity-checked indices despite enabled flags.
	Local W.ActorInstance = MakeActor(MaskAll, "", "Warrior")
	Assert(ActorHasSlot(W\Actor, SlotI_Weapon, staff) = False)
	; FLAG-FOR-HUMAN: same index-vs-name confusion as the race fork --
	; Ring4/Amulet1/Amulet2 (indices 11..13) skip the class check too.
	Assert(ActorHasSlot(W\Actor, SlotI_Ring4, staff) = True)

	; Matching class (case-insensitive) falls through to the flag check --
	; unlike the race match, it does NOT override a disabled slot.
	Local M.ActorInstance = MakeActor(MaskAll, "", "mAgE")
	Assert(ActorHasSlot(M\Actor, SlotI_Weapon, staff) = True)
	M\Actor\InventorySlots = 0
	Assert(ActorHasSlot(M\Actor, SlotI_Weapon, staff) = False)

	ClearAll()
End Test

; A matching ExclusiveRace$ returns True before the ExclusiveClass$ check
; ever runs: an item exclusive to BOTH race Elf and class Mage is granted to
; an Elf Warrior.
; FLAG-FOR-HUMAN: race-match short-circuits the class restriction (and the
; disabled-slot flag check). If race+class exclusivity is meant to be AND-ed,
; this fork is wrong; pinned as current behavior.
Test testActorHasSlotRaceMatchShortCircuitsClassCheck()
	ClearAll()
	Local relic.Item = SeedTypedItem("Elf Mage Relic", I_Weapon, 0)
	relic\ExclusiveRace$ = "Elf"
	relic\ExclusiveClass$ = "Mage"
	Local A.ActorInstance = MakeActor(MaskAll, "Elf", "Warrior")

	Assert(ActorHasSlot(A\Actor, SlotI_Weapon, relic) = True)

	ClearAll()
End Test

; ---------------------------------------------------------------------------
; SlotsMatch: item-type -> slot-index mapping (Inventories.bb ~line 275)
; ---------------------------------------------------------------------------

; Backpack indices accept any item type unconditionally.
Test testSlotsMatchBackpackAcceptsAnything()
	ClearAll()
	Local potion.Item = SeedTypedItem("Potion", I_Potion, 0)
	Local sword.Item = SeedTypedItem("Sword", I_Weapon, 0)

	Assert(SlotsMatch(potion, SlotI_Backpack) = True)
	Assert(SlotsMatch(potion, Slots_Inventory) = True)
	Assert(SlotsMatch(sword, SlotI_Backpack) = True)

	ClearAll()
End Test

; Weapons fit only the weapon index. Non-equippable types (potion etc.) fit
; no equip index at all.
Test testSlotsMatchWeaponAndNonEquippables()
	ClearAll()
	Local sword.Item = SeedTypedItem("Sword", I_Weapon, 0)
	Local potion.Item = SeedTypedItem("Potion", I_Potion, 0)
	Local i

	Assert(SlotsMatch(sword, SlotI_Weapon) = True)
	For i = SlotI_Shield To SlotI_Amulet2
		Assert(SlotsMatch(sword, i) = False)
	Next
	For i = SlotI_Weapon To SlotI_Amulet2
		Assert(SlotsMatch(potion, i) = False)
	Next

	ClearAll()
End Test

; Armour routes by SlotType (the Slot_* NAME constants) to exactly one
; armour index; an armour item with an unmapped SlotType fits no equip slot.
Test testSlotsMatchArmourBySlotType()
	ClearAll()
	Local shield.Item = SeedTypedItem("Shield", I_Armour, Slot_Shield)
	Local chest.Item = SeedTypedItem("Cuirass", I_Armour, Slot_Chest)
	Local boots.Item = SeedTypedItem("Boots", I_Armour, Slot_Feet)
	Local odd.Item = SeedTypedItem("OddArmour", I_Armour, Slot_Weapon)
	Local i

	Assert(SlotsMatch(shield, SlotI_Shield) = True)
	Assert(SlotsMatch(shield, SlotI_Chest) = False)
	Assert(SlotsMatch(chest, SlotI_Chest) = True)
	Assert(SlotsMatch(chest, SlotI_Weapon) = False)
	Assert(SlotsMatch(boots, SlotI_Feet) = True)
	Assert(SlotsMatch(boots, SlotI_Legs) = False)

	; Armour whose SlotType is Slot_Weapon hits no Case arm: rejected at
	; every equip index (the weapon index included -- armour never goes
	; in the weapon slot).
	For i = SlotI_Weapon To SlotI_Amulet2
		Assert(SlotsMatch(odd, i) = False)
	Next

	ClearAll()
End Test

; I_Ring items split on SlotType: Slot_Ring -> the four ring indices;
; ANY other SlotType -> the two amulet indices (there is no explicit
; Slot_Amulet test -- "not ring" means amulet).
Test testSlotsMatchRingVersusAmulet()
	ClearAll()
	Local ring.Item = SeedTypedItem("Ring", I_Ring, Slot_Ring)
	Local amulet.Item = SeedTypedItem("Amulet", I_Ring, Slot_Amulet)
	Local oddRing.Item = SeedTypedItem("OddRing", I_Ring, 0)

	Assert(SlotsMatch(ring, SlotI_Ring1) = True)
	Assert(SlotsMatch(ring, SlotI_Ring4) = True)
	Assert(SlotsMatch(ring, SlotI_Amulet1) = False)
	Assert(SlotsMatch(ring, SlotI_Weapon) = False)

	Assert(SlotsMatch(amulet, SlotI_Amulet1) = True)
	Assert(SlotsMatch(amulet, SlotI_Amulet2) = True)
	Assert(SlotsMatch(amulet, SlotI_Ring1) = False)

	; Pin the catch-all: an I_Ring item with SlotType 0 (or anything that
	; isn't Slot_Ring) behaves as an amulet.
	Assert(SlotsMatch(oddRing, SlotI_Amulet1) = True)
	Assert(SlotsMatch(oddRing, SlotI_Ring1) = False)

	ClearAll()
End Test

; ---------------------------------------------------------------------------
; InventorySwap: equip / unequip round trip + rejection paths
; ---------------------------------------------------------------------------

; Equipping a weapon from the backpack into the empty weapon slot, then
; unequipping it back, leaves the inventory exactly as it started.
Test testSwapEquipUnequipRoundTrip()
	ClearAll()
	Local sword.Item = SeedTypedItem("Sword", I_Weapon, 0)
	Local A.ActorInstance = MakeActor(MaskAll)
	Local Inst.ItemInstance = GiveItem(A, SlotI_Backpack, sword, 1)

	; Equip (backpack 14 -> weapon 0).
	Assert(InventorySwap(A, SlotI_Backpack, SlotI_Weapon, 0, False) = True)
	Assert(A\Inventory\Items[SlotI_Weapon] = Inst)
	Assert(A\Inventory\Amounts[SlotI_Weapon] = 1)
	Assert(A\Inventory\Items[SlotI_Backpack] = Null)
	Assert(A\Inventory\Amounts[SlotI_Backpack] = 0)

	; Unequip (weapon 0 -> backpack 14).
	Assert(InventorySwap(A, SlotI_Weapon, SlotI_Backpack, 0, False) = True)
	Assert(A\Inventory\Items[SlotI_Backpack] = Inst)
	Assert(A\Inventory\Amounts[SlotI_Backpack] = 1)
	Assert(A\Inventory\Items[SlotI_Weapon] = Null)
	Assert(A\Inventory\Amounts[SlotI_Weapon] = 0)

	ClearAll()
End Test

; A weapon cannot be swapped into a non-weapon equip index (SlotsMatch
; rejects), and the inventory is left untouched.
Test testSwapRejectsWrongEquipSlot()
	ClearAll()
	Local sword.Item = SeedTypedItem("Sword", I_Weapon, 0)
	Local A.ActorInstance = MakeActor(MaskAll)
	Local Inst.ItemInstance = GiveItem(A, SlotI_Backpack, sword, 1)

	Assert(InventorySwap(A, SlotI_Backpack, SlotI_Chest, 0, False) = False)
	Assert(InventorySwap(A, SlotI_Backpack, SlotI_Ring1, 0, False) = False)
	Assert(A\Inventory\Items[SlotI_Backpack] = Inst)
	Assert(A\Inventory\Amounts[SlotI_Backpack] = 1)
	Assert(A\Inventory\Items[SlotI_Chest] = Null)

	; Out-of-range slot indices are rejected outright.
	Assert(InventorySwap(A, -1, SlotI_Weapon, 0, False) = False)
	Assert(InventorySwap(A, SlotI_Backpack, Slots_Inventory + 1, 0, False) = False)

	; Empty source slot is rejected.
	Assert(InventorySwap(A, SlotI_Backpack + 1, SlotI_Weapon, 0, False) = False)

	ClearAll()
End Test

; Swapping into an OCCUPIED matching slot exchanges the two items (this is
; how "equip while something is already equipped" displaces the old item
; back to the backpack slot the new one came from).
Test testSwapOccupiedSlotExchangesItems()
	ClearAll()
	Local sword.Item = SeedTypedItem("Sword", I_Weapon, 0)
	Local axe.Item = SeedTypedItem("Axe", I_Weapon, 0)
	Local A.ActorInstance = MakeActor(MaskAll)
	Local equipped.ItemInstance = GiveItem(A, SlotI_Weapon, sword, 1)
	Local carried.ItemInstance = GiveItem(A, SlotI_Backpack, axe, 1)

	Assert(InventorySwap(A, SlotI_Backpack, SlotI_Weapon, 0, False) = True)
	Assert(A\Inventory\Items[SlotI_Weapon] = carried)
	Assert(A\Inventory\Items[SlotI_Backpack] = equipped)
	Assert(A\Inventory\Amounts[SlotI_Weapon] = 1)
	Assert(A\Inventory\Amounts[SlotI_Backpack] = 1)

	ClearAll()
End Test

; A stack of more than one item can never enter a non-backpack slot: both
; the explicit Amount > 1 guard and the whole-stack (Amount = 0) guard fire.
Test testSwapRejectsStackIntoEquipSlot()
	ClearAll()
	Local knives.Item = SeedTypedItem("Throwing Knives", I_Weapon, 0)
	knives\Stackable = True
	Local A.ActorInstance = MakeActor(MaskAll)
	Local Inst.ItemInstance = GiveItem(A, SlotI_Backpack, knives, 2)

	; Whole-stack move of 2 into the weapon slot: rejected.
	Assert(InventorySwap(A, SlotI_Backpack, SlotI_Weapon, 0, False) = False)
	; Explicit Amount = 2 into an equip slot: rejected by the early guard.
	Assert(InventorySwap(A, SlotI_Backpack, SlotI_Weapon, 2, False) = False)
	Assert(A\Inventory\Items[SlotI_Backpack] = Inst)
	Assert(A\Inventory\Amounts[SlotI_Backpack] = 2)
	Assert(A\Inventory\Items[SlotI_Weapon] = Null)

	; A single unit (Amount = 1) of the stack IS allowed into the equip
	; slot; the remainder stays behind as a copied instance.
	Assert(InventorySwap(A, SlotI_Backpack, SlotI_Weapon, 1, False) = True)
	Assert(A\Inventory\Amounts[SlotI_Weapon] = 1)
	Assert(A\Inventory\Amounts[SlotI_Backpack] = 1)
	Assert(A\Inventory\Items[SlotI_Weapon] <> Null)
	Assert(A\Inventory\Items[SlotI_Backpack] <> Null)

	ClearAll()
End Test

; Disabled slot flag blocks the swap even when the item type matches.
Test testSwapRejectsDisabledSlot()
	ClearAll()
	Local sword.Item = SeedTypedItem("Sword", I_Weapon, 0)
	; Backpack bit set, weapon bit NOT set.
	Local A.ActorInstance = MakeActor(1 Shl 10)
	Local Inst.ItemInstance = GiveItem(A, SlotI_Backpack, sword, 1)

	Assert(InventorySwap(A, SlotI_Backpack, SlotI_Weapon, 0, False) = False)
	Assert(A\Inventory\Items[SlotI_Backpack] = Inst)

	ClearAll()
End Test

; Partial-amount move between backpack slots splits the stack: the moved
; part lands in the destination as a COPY (a distinct but identical
; instance); moving the whole amount relocates the original instance and
; nulls the source.
Test testSwapPartialAmountSplitsStack()
	ClearAll()
	Local potion.Item = SeedTypedItem("Potion", I_Potion, 0)
	potion\Stackable = True
	Local A.ActorInstance = MakeActor(MaskAll)
	Local Inst.ItemInstance = GiveItem(A, SlotI_Backpack, potion, 5)

	; Move 2 of 5 into the empty next slot.
	Assert(InventorySwap(A, SlotI_Backpack, SlotI_Backpack + 1, 2, False) = True)
	Assert(A\Inventory\Amounts[SlotI_Backpack] = 3)
	Assert(A\Inventory\Amounts[SlotI_Backpack + 1] = 2)
	Assert(A\Inventory\Items[SlotI_Backpack] = Inst)
	Assert(A\Inventory\Items[SlotI_Backpack + 1] <> Null)
	Assert(A\Inventory\Items[SlotI_Backpack + 1] <> Inst)
	Assert(ItemInstancesIdentical(A\Inventory\Items[SlotI_Backpack], A\Inventory\Items[SlotI_Backpack + 1]) = True)

	; Move the remaining 3 entirely: the original instance relocates.
	Assert(InventorySwap(A, SlotI_Backpack, SlotI_Backpack + 2, 3, False) = True)
	Assert(A\Inventory\Items[SlotI_Backpack + 2] = Inst)
	Assert(A\Inventory\Amounts[SlotI_Backpack + 2] = 3)
	Assert(A\Inventory\Items[SlotI_Backpack] = Null)

	; Overdraw and non-positive amounts are rejected (dupe guard).
	Assert(InventorySwap(A, SlotI_Backpack + 1, SlotI_Backpack + 3, 9, False) = False)
	Assert(InventorySwap(A, SlotI_Backpack + 1, SlotI_Backpack + 3, -3, False) = False)
	Assert(A\Inventory\Amounts[SlotI_Backpack + 1] = 2)
	Assert(A\Inventory\Items[SlotI_Backpack + 3] = Null)

	ClearAll()
End Test

; ---------------------------------------------------------------------------
; InventoryAdd: merging identical stacks
; ---------------------------------------------------------------------------

; Merging a stack into an identical stack accumulates the destination and
; frees the drained source instance (the source slot reads Null afterwards
; via Blitz delete-null semantics).
Test testAddMergesIdenticalStacksAndDrainsSource()
	ClearAll()
	Local potion.Item = SeedTypedItem("Potion", I_Potion, 0)
	potion\Stackable = True
	Local A.ActorInstance = MakeActor(MaskAll)
	Local src.ItemInstance = GiveItem(A, SlotI_Backpack, potion, 3)
	Local dst.ItemInstance = GiveItem(A, SlotI_Backpack + 1, potion, 4)

	Assert(InventoryAdd(A, SlotI_Backpack, SlotI_Backpack + 1, 3, False) = True)
	Assert(A\Inventory\Amounts[SlotI_Backpack + 1] = 7)
	Assert(A\Inventory\Items[SlotI_Backpack + 1] = dst)
	Assert(A\Inventory\Amounts[SlotI_Backpack] = 0)
	Assert(A\Inventory\Items[SlotI_Backpack] = Null)

	ClearAll()
End Test

; Rejection paths: non-identical instances, empty destination, equip-slot
; destination, and bad amounts all leave the inventory untouched.
Test testAddRejectionPaths()
	ClearAll()
	Local potion.Item = SeedTypedItem("Potion", I_Potion, 0)
	Local A.ActorInstance = MakeActor(MaskAll)
	Local src.ItemInstance = GiveItem(A, SlotI_Backpack, potion, 3)
	Local dst.ItemInstance = GiveItem(A, SlotI_Backpack + 1, potion, 4)

	; Non-identical (different ItemHealth): rejected.
	dst\ItemHealth = 50
	Assert(InventoryAdd(A, SlotI_Backpack, SlotI_Backpack + 1, 3, False) = False)
	dst\ItemHealth = 100

	; Empty destination: rejected (InventoryAdd only merges into an
	; existing stack; moves into empty slots go through InventorySwap).
	Assert(InventoryAdd(A, SlotI_Backpack, SlotI_Backpack + 2, 3, False) = False)

	; Destination below SlotI_Backpack: rejected (no merging into equip
	; slots).
	Assert(InventoryAdd(A, SlotI_Backpack, SlotI_Weapon, 3, False) = False)

	; Negative / zero / oversized amounts: rejected (dupe guard).
	Assert(InventoryAdd(A, SlotI_Backpack, SlotI_Backpack + 1, -5, False) = False)
	Assert(InventoryAdd(A, SlotI_Backpack, SlotI_Backpack + 1, 0, False) = False)
	Assert(InventoryAdd(A, SlotI_Backpack, SlotI_Backpack + 1, 4, False) = False)

	Assert(A\Inventory\Amounts[SlotI_Backpack] = 3)
	Assert(A\Inventory\Amounts[SlotI_Backpack + 1] = 4)
	Assert(A\Inventory\Items[SlotI_Backpack] = src)
	Assert(A\Inventory\Items[SlotI_Backpack + 1] = dst)

	ClearAll()
End Test

; The 16-bit stack ceiling, exercised against the REAL InventoryAdd (the
; earlier InventoryStackClampTest pinned a replicated copy of this logic):
; only what fits below MaxStackAmount moves; the remainder stays in the
; source; a full destination accepts nothing.
Test testAddCapsAtStackCeilingNonLossy()
	ClearAll()
	Local potion.Item = SeedTypedItem("Potion", I_Potion, 0)
	potion\Stackable = True
	Local A.ActorInstance = MakeActor(MaskAll)
	Local src.ItemInstance = GiveItem(A, SlotI_Backpack, potion, 5000)
	Local dst.ItemInstance = GiveItem(A, SlotI_Backpack + 1, potion, 32000)

	Assert(InventoryAdd(A, SlotI_Backpack, SlotI_Backpack + 1, 5000, False) = True)
	Assert(A\Inventory\Amounts[SlotI_Backpack + 1] = MaxStackAmount)   ; 32767
	Assert(A\Inventory\Amounts[SlotI_Backpack] = 4233)                 ; 5000 - 767
	Assert(A\Inventory\Items[SlotI_Backpack] = src)                    ; source kept

	; Destination now full: nothing moves.
	Assert(InventoryAdd(A, SlotI_Backpack, SlotI_Backpack + 1, 100, False) = False)
	Assert(A\Inventory\Amounts[SlotI_Backpack] = 4233)
	Assert(A\Inventory\Amounts[SlotI_Backpack + 1] = MaxStackAmount)

	ClearAll()
End Test

; ---------------------------------------------------------------------------
; InventoryDrop: decrement + drain
; ---------------------------------------------------------------------------

Test testDropDecrementsAndDrainsSlot()
	ClearAll()
	Local potion.Item = SeedTypedItem("Potion", I_Potion, 0)
	Local A.ActorInstance = MakeActor(MaskAll)
	Local Inst.ItemInstance = GiveItem(A, SlotI_Backpack, potion, 5)

	; Partial drop: amount decremented, instance still in the slot. The
	; return value is Handle(Inst) -- nonzero on success.
	Assert(InventoryDrop(A, SlotI_Backpack, 2, False) <> 0)
	Assert(A\Inventory\Amounts[SlotI_Backpack] = 3)
	Assert(A\Inventory\Items[SlotI_Backpack] = Inst)

	; Dropping the rest empties the slot (Items nulled, instance NOT freed
	; -- the caller owns it for the floor drop).
	Assert(InventoryDrop(A, SlotI_Backpack, 3, False) <> 0)
	Assert(A\Inventory\Amounts[SlotI_Backpack] = 0)
	Assert(A\Inventory\Items[SlotI_Backpack] = Null)

	ClearAll()
End Test

Test testDropRejectionPaths()
	ClearAll()
	Local potion.Item = SeedTypedItem("Potion", I_Potion, 0)
	Local A.ActorInstance = MakeActor(MaskAll)
	Local Inst.ItemInstance = GiveItem(A, SlotI_Backpack, potion, 2)

	; More than held: rejected.
	Assert(InventoryDrop(A, SlotI_Backpack, 3, False) = False)
	; Empty slot: rejected.
	Assert(InventoryDrop(A, SlotI_Backpack + 1, 1, False) = False)
	; Out-of-range slots: rejected.
	Assert(InventoryDrop(A, -1, 1, False) = False)
	Assert(InventoryDrop(A, Slots_Inventory + 1, 1, False) = False)

	Assert(A\Inventory\Amounts[SlotI_Backpack] = 2)
	Assert(A\Inventory\Items[SlotI_Backpack] = Inst)

	ClearAll()
End Test

; ---------------------------------------------------------------------------
; GetArmourLevel / InventoryMass / InventoryHasItem: equipped-range readers
; ---------------------------------------------------------------------------

; Armour level sums ArmourLevel over the Shield..Feet index range only, and
; only for I_Armour items with ItemHealth > 0 (broken armour contributes 0;
; weapons and backpack contents never count).
Test testGetArmourLevelCountsOnlyIntactEquippedArmour()
	ClearAll()
	Local cuirass.Item = SeedTypedItem("Cuirass", I_Armour, Slot_Chest)
	cuirass\ArmourLevel = 5
	Local greaves.Item = SeedTypedItem("Greaves", I_Armour, Slot_Legs)
	greaves\ArmourLevel = 7
	Local sword.Item = SeedTypedItem("Sword", I_Weapon, 0)
	sword\ArmourLevel = 99      ; never counted: weapon slot is outside the loop
	Local spare.Item = SeedTypedItem("Spare Plate", I_Armour, Slot_Chest)
	spare\ArmourLevel = 11      ; never counted: lives in the backpack

	Local A.ActorInstance = MakeActor(MaskAll)
	Local chestInst.ItemInstance = GiveItem(A, SlotI_Chest, cuirass, 1)
	Local legsInst.ItemInstance = GiveItem(A, SlotI_Legs, greaves, 1)
	Local swordInst.ItemInstance = GiveItem(A, SlotI_Weapon, sword, 1)
	Local spareInst.ItemInstance = GiveItem(A, SlotI_Backpack, spare, 1)

	Assert(GetArmourLevel(A\Inventory) = 12)

	; Broken armour (ItemHealth 0) stops contributing.
	legsInst\ItemHealth = 0
	Assert(GetArmourLevel(A\Inventory) = 5)

	ClearAll()
End Test

; InventoryMass multiplies each slot's item Mass by its stack amount across
; ALL slots (equipped + backpack); InventoryHasItem matches the item name
; case-insensitively and sums amounts across slots.
Test testInventoryMassAndHasItem()
	ClearAll()
	Local potion.Item = SeedTypedItem("Potion", I_Potion, 0)
	potion\Mass = 2
	Local sword.Item = SeedTypedItem("Sword", I_Weapon, 0)
	sword\Mass = 5

	Local A.ActorInstance = MakeActor(MaskAll)
	Local p1.ItemInstance = GiveItem(A, SlotI_Backpack, potion, 3)
	Local p2.ItemInstance = GiveItem(A, SlotI_Backpack + 1, potion, 2)
	Local s1.ItemInstance = GiveItem(A, SlotI_Weapon, sword, 1)

	Assert(InventoryMass(A\Inventory) = 15)   ; 2*3 + 2*2 + 5*1

	Assert(InventoryHasItem(A\Inventory, "POTION", 5) = True)
	Assert(InventoryHasItem(A\Inventory, "potion", 6) = False)
	Assert(InventoryHasItem(A\Inventory, "sword", 1) = True)
	Assert(InventoryHasItem(A\Inventory, "shield", 1) = False)

	ClearAll()
End Test
