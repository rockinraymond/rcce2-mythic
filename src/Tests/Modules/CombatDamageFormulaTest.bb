Strict
EnableGC

; ============================================================================
; Gameplay-correctness regression pins for the live combat math in
; src/Modules/GameServer.bb: ActorAttack (melee formulas 1/2/3, gates,
; ranged dispatch), FireProjectile (hit roll, damage, gates), GiveXP
; (direct award, leader redirect, wire echo) and KillActor (faction-rating
; adjust, kill-XP award).
;
; These are PIN-CURRENT-BEHAVIOR tests, not spec tests: every expected
; value below was derived by reading the shipped formula and confirmed by
; running it. Anything that looks wrong is pinned as-is and marked with a
; `FLAG-FOR-HUMAN` comment rather than "fixed" in the assertion.
;
; Unlike the earlier PartyXPSplitTest (which replicated the arithmetic),
; this file Includes the REAL GameServer.bb -- plus the real Items.bb,
; Inventories.bb (real GetArmourLevel math) and Projectiles.bb -- and
; stubs only the world/network surface around them (Actors.bb types,
; RCEnet send, Gooey gadgets, Scripting's ThreadScript). All combat
; arithmetic exercised here is the shipped code.
;
; Randomness policy: ActorAttack rolls Rand() for to-hit (90%), crits
; (10%) and some damage variance. Tests SeedRnd a fixed seed for
; reproducibility, then assert SET MEMBERSHIP of the per-attack HP delta
; (every observed delta must be one of the few values the formula can
; produce) plus occurrence counts over a volley. With 300 swings the
; probability of a class not appearing is < 1e-12, so the tests are
; deterministic in practice without pinning the RNG stream itself.
; ============================================================================

; --- Constants normally provided by Actors.bb (not includable: it pulls
; --- the world/network dependency cascade). Values copied verbatim.
Const AI_Wait        = 0
Const AI_Patrol      = 1
Const AI_Run         = 2
Const AI_Chase       = 3
Const AI_PatrolPause = 4
Const AI_Pet         = 5
Const AI_PetChase    = 6
Const AI_PetWait     = 7

Const Environment_Amphibious = 0
Const Environment_Swim       = 1
Const Environment_Fly        = 2
Const Environment_Walk       = 3

; --- Stat-slot globals normally established by Server.bb at boot.
; Declared BEFORE the Includes so the non-Strict modules bind these
; globals instead of silently auto-declaring function-locals that read 0.
Global HealthStat    = 0
Global StrengthStat  = 1
Global SpeedStat     = 2
Global ToughnessStat = -1
Global EnergyStat    = -1
Global BreathStat    = -1

; --- Combat configuration globals normally declared in Server.bb.
Global CombatDelay = 0
Global CombatFormula = 2
Global CombatRatingAdjust = 10
Global Host = 0

; Pre-declared for the real Logging.bb include below (it supplies WriteLog,
; SafeWriteOpen/Commit and ReadBoundedString$ -- same precedent as
; ReadBoundedStringTest / SafeWriteTest).
Global LogMode = 0
Global MainLog = 0

; --- Packet-type ids. Production values live in Packets.bb (not needed
; --- here); these only have to be DISTINCT so the RCE_Send capture stub
; --- can tell an attack-result packet from an XP packet.
Global P_AttackActor = 200
Global P_XPUpdate    = 201

; --- Online-player chain head normally declared in Actors.bb. GameServer's
; broadcast walk reads it; must be a typed global or the non-Strict
; assignment `AI.ActorInstance = FirstOnlinePlayer` fails to compile.
Global FirstOnlinePlayer.ActorInstance = Null

; ----------------------------------------------------------------------------
; Type stubs for the Actors.bb / ServerAreas.bb / Scripting.bb types that
; GameServer.bb (and Items.bb / Inventories.bb) reference. Field shapes are
; copied from the real definitions so every field access in the included
; modules resolves; only comments were trimmed.
; ----------------------------------------------------------------------------

Type Attributes
	Field Value[39]
	Field Maximum[39]
	Field My_ID
End Type

Type Actor
	Field ID
	Field Race$, Class$, Description$, StartArea$, StartPortal$
	Field Radius#
	Field Scale#
	Field MeshIDs[7]
	Field BloodTexID
	Field Genders
	Field Attributes.Attributes
	Field Resistances[19]
	Field MAnimationSet, FAnimationSet
	Field Playable, Rideable
	Field Aggressiveness
	Field AggressiveRange
	Field TradeMode
	Field Environment
	Field InventorySlots
	Field DefaultDamageType
	Field DefaultFaction
	Field XPMultiplier
	Field PolyCollision
End Type

Type ActorInstance
	Field Actor.Actor
	Field NextInZone.ActorInstance
	Field NextOnlinePlayer.ActorInstance
	Field FirstSlave.ActorInstance
	Field NextSlave.ActorInstance
	Field X#, Y#, Z#
	Field OldX#, OldZ#
	Field DestX#, DestZ#
	Field LastPosUpdateMs%
	Field Yaw#
	Field WalkingBackward
	Field Area$, ServerArea, Account
	Field Name$, Tag$
	Field LastPortal, LastTrigger, LastPortalTime, LastPortalArea
	Field LastPortalAreaName$
	Field TeamID
	Field PartyID, AcceptPending
	Field Gender
	Field EN, CollisionEN, NametagEN
	Field FaceTex, Hair, Beard, BodyTex
	Field Level, XP, XPBarLevel
	Field HomeFaction
	Field FactionRatings[99]
	Field Attributes.Attributes
	Field Resistances[19]
	Field Script$
	Field DeathScript$
	Field Inventory.Inventory
	Field Leader.ActorInstance
	Field NumberOfSlaves
	Field Reputation
	Field Gold
	Field RNID
	Field RuntimeID
	Field SourceSP, CurrentWaypoint, AIMode, AITarget.ActorInstance
	Field Rider.ActorInstance, Mount.ActorInstance
	Field IsRunning, LastAttack
	Field FootstepPlayedThisCycle
	Field ScriptGlobals$[9]
	Field KnownSpells[999]
	Field SpellLevels[999]
	Field MemorisedSpells[9]
	Field SpellCharge[999]
	Field LastSpellFireMs
	Field IsTrading
	Field TradingActor.ActorInstance
	Field TradeResult$
	Field TradeOfferedAmount[31]
	Field Underwater
	Field IgnoreUpdate
	Field WalkingRight
	Field Active
End Type

Type Party
	Field Members
	Field Player.ActorInstance[7]
End Type

Type Area
	Field Name$
	Field WeatherChance[4]
	Field Outdoors
	Field WeatherLink$, WeatherLinkArea.Area
	Field EntryScript$, ExitScript$
	Field TriggerX#[149], TriggerY#[149], TriggerZ#[149], TriggerSize#[149], TriggerScript$[149], TriggerMethod$[149]
	Field WaypointX#[1999], WaypointY#[1999], WaypointZ#[1999]
	Field PrevWaypoint[1999], NextWaypointA[1999], NextWaypointB[1999]
	Field WaypointPause[1999]
	Field PortalName$[99], PortalLinkArea$[99], PortalLinkName$[99]
	Field PortalX#[99], PortalY#[99], PortalZ#[99], PortalSize#[99], PortalYaw#[99]
	Field SpawnActor[999], SpawnWaypoint[999], SpawnSize#[999], SpawnScript$[999], SpawnActorScript$[999], SpawnDeathScript$[999]
	Field SpawnFrequency[999], SpawnMax[999], SpawnRange#[999]
	Field PvP
	Field Gravity
	Field Instances.AreaInstance[99]
	Field FirstWater.ServerWater
End Type

Type ServerWater
	Field Area.Area
	Field X#, Y#, Z#
	Field Width#, Depth#
	Field Damage, DamageType
	Field NextWater.ServerWater
End Type

Type AreaInstance
	Field Area.Area
	Field ID
	Field FirstInZone.ActorInstance
	Field CurrentWeather, CurrentWeatherTime
	Field SpawnLast[999], Spawned[999]
End Type

Type ActorEffect
	Field Name$
	Field Owner.ActorInstance
	Field Attributes.Attributes
	Field CreatedTime, Length
	Field IconTexID
End Type

Type PausedScript
	Field Reason%
	Field ReasonActor.ActorInstance, ReasonContextActor.ActorInstance, ReasonKillActor.Actor, ReasonItem$, ReasonAmount%
	Field ReasonCount%
	Field S.ScriptInstance
End Type

Type ScriptInstance
	Field Name$
	Field WaitResult$
End Type

; ----------------------------------------------------------------------------
; Function stubs
; ----------------------------------------------------------------------------

; RCE wire-format helpers, verbatim from RCEnet.bb except for a private Bank
; (same approach as ItemsTest.bb).
Global CombatTest_ConvertBank.BBBank = CreateBank(8)

Function RCE_IntFromStr(Dat$)
	PokeInt CombatTest_ConvertBank, 0, 0
	Local i
	For i = 1 To Len(Dat$)
		PokeByte CombatTest_ConvertBank, i - 1, Asc(Mid$(Dat$, i, 1))
	Next
	Return PeekInt(CombatTest_ConvertBank, 0)
End Function

Function RCE_StrFromInt$(Num, Length = 4)
	PokeInt CombatTest_ConvertBank, 0, Num
	Local Dat$ = ""
	Local i
	For i = Length - 1 To 0 Step -1
		Dat$ = Chr$(PeekByte(CombatTest_ConvertBank, i)) + Dat$
	Next
	Return Dat$
End Function

; Floats only ride packets these tests never decode; a fixed-width filler
; keeps the (unexercised) broadcast code paths compiling.
Function RCE_StrFromFloat$(Num#)
	Return Chr$(0) + Chr$(0) + Chr$(0) + Chr$(0)
End Function

; --- RCE_Send capture stub. Records the packets the combat code emits so
; tests can pin the wire echo of the damage actually applied.
Global Cap_SendCount = 0
Global Cap_LastType = -1
Global Cap_LastData$ = ""
Global Cap_AttackHCount = 0
Global Cap_AttackH$ = ""     ; payload after the "H" prefix
Global Cap_XPCount = 0
Global Cap_XPData$ = ""      ; payload after the "M" prefix

Function RCE_Send(Connection, Destination, MessageType, MessageData$, ReliableFlag = 0, PlayerFrom = 0, DoNotUse = 0, ConfirmID = -1)
	Cap_SendCount = Cap_SendCount + 1
	Cap_LastType = MessageType
	Cap_LastData$ = MessageData$
	If MessageType = P_AttackActor And Left$(MessageData$, 1) = "H"
		Cap_AttackHCount = Cap_AttackHCount + 1
		Cap_AttackH$ = Mid$(MessageData$, 2)
	EndIf
	If MessageType = P_XPUpdate And Left$(MessageData$, 1) = "M"
		Cap_XPCount = Cap_XPCount + 1
		Cap_XPData$ = Mid$(MessageData$, 2)
	EndIf
	Return True
End Function

Function SendQueued(Connection, Destination, PacketType, Pa$, ReliableFlag = False, PlayerFrom = 0)
End Function

Function SendPartyUpdate(AI.ActorInstance)
End Function

; --- Scripting capture stub (real ThreadScript spawns a BVM context).
Global Cap_ScriptCount = 0
Global Cap_LastScript$ = ""
Global Cap_LastScriptFunc$ = ""

Function ThreadScript(Name$, Func$, Actor%, CActor%, Param$ = "", Privileged% = 0)
	Cap_ScriptCount = Cap_ScriptCount + 1
	Cap_LastScript$ = Name$
	Cap_LastScriptFunc$ = Func$
End Function

; --- Actors.bb helpers
; Real UpdateAttribute (Server.bb) also clamps and echoes a packet; the
; only GameServer caller is the (unexercised) underwater-damage tick.
Function UpdateAttribute(AI.ActorInstance, Att, Value)
	AI\Attributes\Value[Att] = Value
End Function

Function FreeActorScripts(A.ActorInstance)
End Function

Global Cap_FreeCount = 0
Function FreeActorInstance(A.ActorInstance)
	; Count only: tests still need to read the killer/victim afterwards;
	; teardown deletes every ActorInstance.
	Cap_FreeCount = Cap_FreeCount + 1
End Function

Function ActorInstanceToString$(A.ActorInstance)
	Return ""
End Function

; Verbatim from Actors.bb (bit test used by Inventories.bb's ActorHasSlot).
Function GetFlag(TheInt, Flag)
	Return (TheInt Shr Flag) And 1
End Function

; --- Language stub (Language.bb is not includable here).
Function LanguageString$(key$)
	Return key
End Function

; --- Gooey gadget stubs. Only CreateGameWindow / SetArea's players-list
; bookkeeping touch these and no test calls either; they exist so
; GameServer.bb compiles.
Function Desktop()
	Return 0
End Function

Function CreateWindow(Title$, X, Y, W, H, Parent, Flags = 0)
	Return 0
End Function

Function CreateLabel(Text$, X, Y, W, H, Parent)
	Return 0
End Function

Function CreateButton(Text$, X, Y, W, H, Parent)
	Return 0
End Function

Function CreateTextField(X, Y, W, H, Parent)
	Return 0
End Function

Function CreateComboBox(X, Y, W, H, Parent)
	Return 0
End Function

Function CreateListBox(X, Y, W, H, Parent)
	Return 0
End Function

Function AddGadgetItem(Gadget, Text$, Selected = False)
	Return 0
End Function

Function AddListBoxItem(Gadget, Text$)
	Return 0
End Function

Function CountGadgetItems(Gadget)
	Return 0
End Function

Function GadgetItemText$(Gadget, Index)
	Return ""
End Function

Function RemoveGadgetItem(Gadget, Index)
	Return 0
End Function

; ----------------------------------------------------------------------------
; The real modules under test
; ----------------------------------------------------------------------------
Include "Modules\Logging.bb"
Include "Modules\Items.bb"
Include "Modules\Inventories.bb"
Include "Modules\Projectiles.bb"
Include "Modules\GameServer.bb"

; ----------------------------------------------------------------------------
; Test helpers
; ----------------------------------------------------------------------------

; Builds a combat-ready actor instance: passive template, zero radius,
; neutral (100) resistance to every damage type, faction ratings 0
; (allowed to fight everyone -- the gate blocks at > 150), no area
; (ServerArea = 0 keeps every Object.AreaInstance lookup Null so the
; broadcast loops are skipped, per the established stale-handle pattern).
Function MakeCombatant.ActorInstance(rnid, hp, strength)
	Local Act.Actor = New Actor()
	Act\Aggressiveness = 0
	Act\Radius# = 0.0
	Act\XPMultiplier = 10
	Local A.ActorInstance = New ActorInstance()
	A\Actor = Act
	A\Attributes = New Attributes()
	A\Attributes\Value[HealthStat] = hp
	A\Attributes\Maximum[HealthStat] = hp
	A\Attributes\Value[StrengthStat] = strength
	A\Inventory = New Inventory()
	A\RNID = rnid
	A\RuntimeID = 1
	A\ServerArea = 0
	A\SourceSP = -1
	A\Level = 1
	Local i
	For i = 0 To 19
		A\Resistances[i] = 100
	Next
	Return A
End Function

Function GiveWeapon.ItemInstance(A.ActorInstance, wtype, dmg, dmgtype)
	Local It.Item = CreateItem()
	It\ItemType = I_Weapon
	It\WeaponType = wtype
	It\WeaponDamage = dmg
	It\WeaponDamageType = dmgtype
	Local Inst.ItemInstance = CreateItemInstance(It)
	Inst\ItemHealth = 100
	A\Inventory\Items[SlotI_Weapon] = Inst
	Return Inst
End Function

Function GiveArmour.ItemInstance(A.ActorInstance, level, slot)
	Local It.Item = CreateItem()
	It\ItemType = I_Armour
	It\ArmourLevel = level
	Local Inst.ItemInstance = CreateItemInstance(It)
	Inst\ItemHealth = 100
	A\Inventory\Items[slot] = Inst
	Return Inst
End Function

Function ResetWorld()
	Delete Each PendingKill
	Delete Each ActorEffect
	Delete Each PausedScript
	Delete Each ScriptInstance
	Delete Each Party
	Delete Each ActorInstance
	Delete Each Actor
	Delete Each ItemInstance
	Delete Each DroppedItem
	Delete Each Item
	Delete Each Attributes
	Delete Each Inventory
	Delete Each Projectile
	Delete Each ServerWater
	Delete Each AreaInstance
	Delete Each Area
	CombatFormula = 2
	ToughnessStat = -1
	EnergyStat = -1
	BreathStat = -1
	WeaponDamage = False
	ArmourDamage = False
	CombatRatingAdjust = 10
	Cap_SendCount = 0
	Cap_LastType = -1
	Cap_LastData$ = ""
	Cap_AttackHCount = 0
	Cap_AttackH$ = ""
	Cap_XPCount = 0
	Cap_XPData$ = ""
	Cap_ScriptCount = 0
	Cap_LastScript$ = ""
	Cap_LastScriptFunc$ = ""
	Cap_FreeCount = 0
End Function

; --- Volley runner. Performs N ActorAttack calls and classifies every
; per-attack HP delta against the (at most two) values the formula can
; produce: V1 = normal hit, V2 = critical hit, 0 = miss. Any other delta
; lands in G_SawOther, which every membership test asserts to be 0.
; Also cross-checks the wire echo: the "H" packet sent to the attacker
; carries Damage + 1, i.e. delta + 1 on a hit and 0 on a miss.
Global G_SawZero = 0
Global G_SawVal1 = 0
Global G_SawVal2 = 0
Global G_SawOther = 0
Global G_WireMismatch = 0
Global G_WireDTypeBad = 0

Function RunVolley(A1.ActorInstance, A2.ActorInstance, N, V1, V2, expectDType)
	G_SawZero = 0
	G_SawVal1 = 0
	G_SawVal2 = 0
	G_SawOther = 0
	G_WireMismatch = 0
	G_WireDTypeBad = 0
	Local i, hpBefore, delta, wireDmg, wireType
	For i = 1 To N
		hpBefore = A2\Attributes\Value[HealthStat]
		Cap_AttackH$ = ""
		ActorAttack(A1, A2)
		delta = hpBefore - A2\Attributes\Value[HealthStat]
		If delta = 0
			G_SawZero = G_SawZero + 1
		ElseIf delta = V1
			G_SawVal1 = G_SawVal1 + 1
		ElseIf delta = V2
			G_SawVal2 = G_SawVal2 + 1
		Else
			G_SawOther = G_SawOther + 1
		EndIf
		; Wire echo: payload = RuntimeID(2) + Damage+1(2) + DamageType(1)
		wireDmg = RCE_IntFromStr(Mid$(Cap_AttackH$, 3, 2))
		If delta = 0
			If wireDmg <> 0 Then G_WireMismatch = G_WireMismatch + 1
		Else
			If wireDmg <> delta + 1 Then G_WireMismatch = G_WireMismatch + 1
			wireType = RCE_IntFromStr(Mid$(Cap_AttackH$, 5, 1))
			If wireType <> expectDType Then G_WireDTypeBad = G_WireDTypeBad + 1
		EndIf
	Next
End Function

; Range-membership variant for the formulas with Rand() damage variance
; (formula 1's strength adjustment). Normal hits must land in [lo, hi];
; crits are double the pre-armour damage, so with zero armour they are
; the EVEN values in [2*lo, 2*hi].
Function RunVolleyRange(A1.ActorInstance, A2.ActorInstance, N, lo, hi)
	G_SawZero = 0
	G_SawVal1 = 0
	G_SawVal2 = 0
	G_SawOther = 0
	Local i, hpBefore, delta
	For i = 1 To N
		hpBefore = A2\Attributes\Value[HealthStat]
		ActorAttack(A1, A2)
		delta = hpBefore - A2\Attributes\Value[HealthStat]
		If delta = 0
			G_SawZero = G_SawZero + 1
		ElseIf delta >= lo And delta <= hi
			G_SawVal1 = G_SawVal1 + 1
		ElseIf delta >= 2 * lo And delta <= 2 * hi And (delta Mod 2) = 0
			G_SawVal2 = G_SawVal2 + 1
		Else
			G_SawOther = G_SawOther + 1
		EndIf
	Next
End Function

; ============================================================================
; ActorAttack -- gate behavior (deterministic)
; ============================================================================

; Dead or Null targets are rejected before any damage math runs (the
; double-kill / use-after-free guard).
Test testActorAttackRejectsNullAndDeadTarget()
	ResetWorld()
	Local A1.ActorInstance = MakeCombatant(0, 100, 8)
	Local A2.ActorInstance = MakeCombatant(0, 0, 8)
	Assert(ActorAttack(A1, A2) = False)
	Assert(A2\Attributes\Value[HealthStat] = 0)
	Assert(ActorAttack(A1, Null) = False)
	Assert(ActorAttack(Null, A2) = False)
	ResetWorld()
End Test

; Aggressiveness 3 = "no combat" blocks the attack from either side.
Test testActorAttackAggressivenessThreeBlocksEitherSide()
	ResetWorld()
	SeedRnd(101)
	Local A1.ActorInstance = MakeCombatant(0, 1000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000, 8)
	A1\Actor\Aggressiveness = 3
	Assert(ActorAttack(A1, A2) = False)
	A1\Actor\Aggressiveness = 0
	A2\Actor\Aggressiveness = 3
	Assert(ActorAttack(A1, A2) = False)
	Assert(A2\Attributes\Value[HealthStat] = 1000)
	ResetWorld()
End Test

; Faction gate: a rating ABOVE 150 with the target's home faction blocks
; the attack; exactly 150 still allows it. (Scale: 0..200, 100 = neutral,
; 150 = "50%".)
Test testActorAttackFactionGateBoundaryIs150()
	ResetWorld()
	SeedRnd(102)
	Local A1.ActorInstance = MakeCombatant(0, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	A2\HomeFaction = 4
	A1\FactionRatings[4] = 151
	Assert(ActorAttack(A1, A2) = False)
	A1\FactionRatings[4] = 150
	Assert(ActorAttack(A1, A2) = True)
	ResetWorld()
End Test

; Melee range gate: 7.0 + both radii, compared against squared distance.
Test testActorAttackMeleeRangeGate()
	ResetWorld()
	SeedRnd(103)
	Local A1.ActorInstance = MakeCombatant(0, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	A2\X# = 100.0
	Assert(ActorAttack(A1, A2) = False)
	A2\X# = 0.0
	Assert(ActorAttack(A1, A2) = True)
	ResetWorld()
End Test

; A ranged weapon with 0 item health refuses the attack outright.
Test testActorAttackBrokenRangedWeaponRefuses()
	ResetWorld()
	SeedRnd(104)
	Local A1.ActorInstance = MakeCombatant(5, 1000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000, 8)
	Local W.ItemInstance = GiveWeapon(A1, W_Ranged, 10, 2)
	W\ItemHealth = 0
	Assert(ActorAttack(A1, A2) = False)
	Assert(A2\Attributes\Value[HealthStat] = 1000)
	ResetWorld()
End Test

; A successful attack on a defensive (Aggressiveness 1) NPC makes it
; target the attacker and enter chase mode.
Test testActorAttackAngersDefensiveNPC()
	ResetWorld()
	SeedRnd(105)
	Local A1.ActorInstance = MakeCombatant(0, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(-1, 1000000, 8)
	A2\Actor\Aggressiveness = 1
	Assert(ActorAttack(A1, A2) = True)
	Assert(A2\AITarget = A1)
	Assert(A2\AIMode = AI_Chase)
	ResetWorld()
End Test

; An aggressive (2) NPC that ALREADY has a target keeps it.
Test testActorAttackKeepsAggressiveNPCExistingTarget()
	ResetWorld()
	SeedRnd(106)
	Local A1.ActorInstance = MakeCombatant(0, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(-1, 1000000, 8)
	Local Other.ActorInstance = MakeCombatant(0, 1000000, 8)
	A2\Actor\Aggressiveness = 2
	A2\AITarget = Other
	A2\AIMode = AI_Chase
	Assert(ActorAttack(A1, A2) = True)
	Assert(A2\AITarget = Other)
	ResetWorld()
End Test

; ============================================================================
; ActorAttack -- CombatFormula 2 damage pipeline (weapon damage is a
; fixed base, so per-attack deltas are exactly {miss 0, W - AP, 2W - AP})
; ============================================================================

; W=20 weapon vs armour 3 + resistance 108 (AP = 3 + (108-100) = 11):
; hit = 9, crit = 29. The crit value 29 (= 2*20 - 11, not 2*(20-11) = 18)
; pins that the critical DOUBLING IS APPLIED BEFORE the armour subtraction.
; Also pins the real GetArmourLevel + resistance composition and the wire
; echo (packet damage field = applied delta + 1; miss echoes 0).
Test testFormula2HitIs9Crit29WithArmour3Resist108()
	ResetWorld()
	SeedRnd(2001)
	CombatFormula = 2
	Local A1.ActorInstance = MakeCombatant(5, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	GiveWeapon(A1, W_OneHand, 20, 2)
	GiveArmour(A2, 3, SlotI_Chest)
	A2\Resistances[2] = 108
	RunVolley(A1, A2, 300, 9, 29, 2)
	Assert(G_SawOther = 0)
	Assert(G_SawVal1 > 0)    ; normal hits occurred
	Assert(G_SawVal2 > 0)    ; crits occurred (P(none in 300) ~ 5e-13)
	Assert(G_SawZero > 0)    ; misses occurred (10% to-hit failure)
	Assert(G_WireMismatch = 0)
	Assert(G_WireDTypeBad = 0)
	ResetWorld()
End Test

; Massive resistance floors every successful hit at exactly 1 damage.
Test testFormula2MinimumDamageFloorIsOne()
	ResetWorld()
	SeedRnd(2002)
	CombatFormula = 2
	Local A1.ActorInstance = MakeCombatant(5, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	GiveWeapon(A1, W_OneHand, 20, 2)
	A2\Resistances[2] = 190
	RunVolley(A1, A2, 300, 1, -999, 2)
	Assert(G_SawOther = 0)
	Assert(G_SawVal1 > 0)
	Assert(G_WireMismatch = 0)
	ResetWorld()
End Test

; Resistance BELOW 100 amplifies damage (negative AP): resist 60 makes a
; W=20 hit deal 60 and a crit deal 80.
Test testFormula2LowResistanceAmplifies()
	ResetWorld()
	SeedRnd(2003)
	CombatFormula = 2
	Local A1.ActorInstance = MakeCombatant(5, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	GiveWeapon(A1, W_OneHand, 20, 2)
	A2\Resistances[2] = 60
	RunVolley(A1, A2, 300, 60, 80, 2)
	Assert(G_SawOther = 0)
	Assert(G_SawVal1 > 0)
	Assert(G_SawVal2 > 0)
	Assert(G_WireMismatch = 0)
	ResetWorld()
End Test

; When a Toughness attribute exists (ToughnessStat > -1), formula 2 adds
; Toughness/8 to AP: Toughness 40 -> +5, so W=20 hits for 15, crits 35.
Test testFormula2ToughnessAddsEighthToArmour()
	ResetWorld()
	SeedRnd(2004)
	CombatFormula = 2
	ToughnessStat = 5
	Local A1.ActorInstance = MakeCombatant(5, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	GiveWeapon(A1, W_OneHand, 20, 2)
	A2\Attributes\Value[5] = 40
	RunVolley(A1, A2, 300, 15, 35, 2)
	Assert(G_SawOther = 0)
	Assert(G_SawVal1 > 0)
	Assert(G_SawVal2 > 0)
	ResetWorld()
End Test

; ============================================================================
; ActorAttack -- CombatFormula 1 strength adjustment
; ============================================================================

; Strength below weapon damage PENALISES: Damage = W - Rand(5,8), so with
; W=20 and zero armour the normal hits are 12..15 and crits the even
; values 24..30.
Test testFormula1WeakerStrengthSubtracts5to8()
	ResetWorld()
	SeedRnd(1001)
	CombatFormula = 1
	Local A1.ActorInstance = MakeCombatant(5, 1000000, 1)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	GiveWeapon(A1, W_OneHand, 20, 2)
	RunVolleyRange(A1, A2, 300, 12, 15)
	Assert(G_SawOther = 0)
	Assert(G_SawVal1 > 0)
	Assert(G_SawVal2 > 0)
	ResetWorld()
End Test

; Strength above weapon damage BONUSES: Damage = W + Rand(5,8) -> 25..28
; normal, 50..56 even crits.
Test testFormula1StrongerStrengthAdds5to8()
	ResetWorld()
	SeedRnd(1002)
	CombatFormula = 1
	Local A1.ActorInstance = MakeCombatant(5, 1000000, 100)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	GiveWeapon(A1, W_OneHand, 20, 2)
	RunVolleyRange(A1, A2, 300, 25, 28)
	Assert(G_SawOther = 0)
	Assert(G_SawVal1 > 0)
	Assert(G_SawVal2 > 0)
	ResetWorld()
End Test

; ============================================================================
; ActorAttack -- CombatFormula 3 (multiplied) armour handling
; ============================================================================

; Formula 3 without a Toughness attribute SQUARES the armour points
; (`AP = AP * AP`). W=4 * S=5 = 20 base; armour 3 squares to 9, so hits
; deal 11 and crits 31.
Test testFormula3SquaresArmourWithoutToughness()
	ResetWorld()
	SeedRnd(3001)
	CombatFormula = 3
	Local A1.ActorInstance = MakeCombatant(5, 1000000, 5)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	GiveWeapon(A1, W_OneHand, 4, 2)
	GiveArmour(A2, 3, SlotI_Chest)
	RunVolley(A1, A2, 300, 11, 31, 2)
	Assert(G_SawOther = 0)
	Assert(G_SawVal1 > 0)
	Assert(G_SawVal2 > 0)
	ResetWorld()
End Test

; FLAG-FOR-HUMAN: under CombatFormula 3 with no Toughness attribute,
; LOW resistance (a vulnerability, resist < 100) makes the negative AP
; positive when squared, so a VULNERABLE target takes only the minimum
; 1 damage. Resist 60 -> AP = -40 -> AP*AP = 1600 -> every hit floors
; to 1. Formulas 1/2 amplify damage for the same setup (see
; testFormula2LowResistanceAmplifies); formula 3 inverts the meaning of
; vulnerability. Pinned as shipped.
Test testFormula3VulnerabilityFloorsToOne()
	ResetWorld()
	SeedRnd(3002)
	CombatFormula = 3
	Local A1.ActorInstance = MakeCombatant(5, 1000000, 5)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	GiveWeapon(A1, W_OneHand, 4, 2)
	A2\Resistances[2] = 60
	RunVolley(A1, A2, 300, 1, -999, 2)
	Assert(G_SawOther = 0)   ; FLAG-FOR-HUMAN: all hits = 1 dmg vs a VULNERABLE target
	Assert(G_SawVal1 > 0)
	ResetWorld()
End Test

; ============================================================================
; ActorAttack -- equipment wear
; ============================================================================

; FLAG-FOR-HUMAN: the "Damage armour" block in ActorAttack wears down the
; ATTACKER's (A1's) equipped armour, not the defender's (A2's), even
; though A2's armour is what absorbed the blow. Pinned as shipped: after
; 300 swings with ArmourDamage on, the defender's chest piece is still at
; exactly 100 health while the attacker's has lost some (1-in-5 chance
; per swing; P(zero losses in 300) ~ 1e-29).
Test testArmourWearHitsAttackerNotDefender()
	ResetWorld()
	SeedRnd(4001)
	CombatFormula = 2
	ArmourDamage = True
	Local A1.ActorInstance = MakeCombatant(5, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	GiveWeapon(A1, W_OneHand, 20, 2)
	Local AttackerChest.ItemInstance = GiveArmour(A1, 3, SlotI_Chest)
	Local DefenderChest.ItemInstance = GiveArmour(A2, 3, SlotI_Chest)
	Local i
	For i = 1 To 300
		ActorAttack(A1, A2)
	Next
	Assert(DefenderChest\ItemHealth = 100)              ; FLAG-FOR-HUMAN
	Assert(AttackerChest\ItemHealth < 100)              ; FLAG-FOR-HUMAN
	Assert(AttackerChest\ItemHealth > 0)
	ResetWorld()
End Test

; Weapon wear: with WeaponDamage on, the attacker's weapon loses health at
; the same 1-in-5 rate.
Test testWeaponWearDecrementsAttackerWeapon()
	ResetWorld()
	SeedRnd(4002)
	CombatFormula = 2
	WeaponDamage = True
	Local A1.ActorInstance = MakeCombatant(5, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	Local W.ItemInstance = GiveWeapon(A1, W_OneHand, 20, 2)
	Local i
	For i = 1 To 100
		ActorAttack(A1, A2)
	Next
	Assert(W\ItemHealth < 100)
	Assert(W\ItemHealth > 0)
	ResetWorld()
End Test

; ============================================================================
; ActorAttack -- ranged dispatch through the real ProjectileList
; ============================================================================

; A working ranged weapon in range routes through FireProjectile (with a
; 100% hit-chance projectile the target's HP must drop) and stamps
; LastAttack.
Test testActorAttackRangedDispatchFiresProjectile()
	ResetWorld()
	SeedRnd(5001)
	CombatFormula = 2
	Local A1.ActorInstance = MakeCombatant(5, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	Local W.ItemInstance = GiveWeapon(A1, W_Ranged, 10, 2)
	W\Item\Range# = 50.0
	Local P.Projectile = CreateProjectile()
	P\HitChance = 100
	P\Damage = 10
	P\DamageType = 2
	W\Item\RangedProjectile = P\ID
	Assert(ActorAttack(A1, A2) = True)
	Assert(A2\Attributes\Value[HealthStat] < 1000000)
	ResetWorld()
End Test

; Ranged weapon out of range is rejected.
Test testActorAttackRangedOutOfRange()
	ResetWorld()
	SeedRnd(5002)
	CombatFormula = 2
	Local A1.ActorInstance = MakeCombatant(5, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	Local W.ItemInstance = GiveWeapon(A1, W_Ranged, 10, 2)
	W\Item\Range# = 5.0
	A2\X# = 100.0
	Assert(ActorAttack(A1, A2) = False)
	Assert(A2\Attributes\Value[HealthStat] = 1000000)
	ResetWorld()
End Test

; ============================================================================
; FireProjectile
; ============================================================================

; 100% hit chance vs resist 108 / no armour (AP = 8): every shot lands in
; [max(1, 10-5-8) .. 10+5-8] = [1..7] (Rand(-5,5) variance, min-1 floor),
; and the wire echo to the shooter is delta + 1.
Test testFireProjectileDamageRangeAndWireEcho()
	ResetWorld()
	SeedRnd(6001)
	Local A1.ActorInstance = MakeCombatant(5, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	Local P.Projectile = CreateProjectile()
	P\HitChance = 100
	P\Damage = 10
	P\DamageType = 2
	A2\Resistances[2] = 108
	Local i, hpBefore, delta, wireDmg
	Local bad = 0
	Local wireBad = 0
	For i = 1 To 100
		hpBefore = A2\Attributes\Value[HealthStat]
		Cap_AttackH$ = ""
		FireProjectile(P, A1, A2)
		delta = hpBefore - A2\Attributes\Value[HealthStat]
		If delta < 1 Or delta > 7 Then bad = bad + 1
		wireDmg = RCE_IntFromStr(Mid$(Cap_AttackH$, 3, 2))
		If wireDmg <> delta + 1 Then wireBad = wireBad + 1
	Next
	Assert(bad = 0)
	Assert(wireBad = 0)
	ResetWorld()
End Test

; 0% hit chance never lands (Rand(100) is 1..100, always > 0).
Test testFireProjectileZeroHitChanceNeverLands()
	ResetWorld()
	SeedRnd(6002)
	Local A1.ActorInstance = MakeCombatant(0, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	Local P.Projectile = CreateProjectile()
	P\HitChance = 0
	P\Damage = 10
	P\DamageType = 2
	Local i
	For i = 1 To 100
		FireProjectile(P, A1, A2)
	Next
	Assert(A2\Attributes\Value[HealthStat] = 1000000)
	ResetWorld()
End Test

; FireProjectile applies the same aggressiveness-3 and faction (> 150)
; gates as melee.
Test testFireProjectileGates()
	ResetWorld()
	SeedRnd(6003)
	Local A1.ActorInstance = MakeCombatant(0, 1000000, 8)
	Local A2.ActorInstance = MakeCombatant(0, 1000000, 8)
	Local P.Projectile = CreateProjectile()
	P\HitChance = 100
	P\Damage = 10
	P\DamageType = 2
	A2\Actor\Aggressiveness = 3
	FireProjectile(P, A1, A2)
	Assert(A2\Attributes\Value[HealthStat] = 1000000)
	A2\Actor\Aggressiveness = 0
	A2\HomeFaction = 7
	A1\FactionRatings[7] = 151
	FireProjectile(P, A1, A2)
	Assert(A2\Attributes\Value[HealthStat] = 1000000)
	A1\FactionRatings[7] = 150
	FireProjectile(P, A1, A2)
	Assert(A2\Attributes\Value[HealthStat] < 1000000)
	ResetWorld()
End Test

; A lethal projectile triggers the kill chain: the NPC victim is freed and
; the killer is awarded XP.
Test testFireProjectileLethalHitKills()
	ResetWorld()
	SeedRnd(6004)
	Local A1.ActorInstance = MakeCombatant(9, 1000, 8)
	Local A2.ActorInstance = MakeCombatant(-1, 1, 8)
	A2\Level = 1
	A2\Actor\XPMultiplier = 10
	Local P.Projectile = CreateProjectile()
	P\HitChance = 100
	P\Damage = 200
	P\DamageType = 2
	FireProjectile(P, A1, A2)
	Assert(A2\Attributes\Value[HealthStat] <= 0)
	Assert(Cap_FreeCount = 1)
	Assert(A1\XP > 0)
	ResetWorld()
End Test

; ============================================================================
; GiveXP
; ============================================================================

; Direct award to an un-partied player: XP adds, the LevelUp script is
; spawned, and the P_XPUpdate "M" packet echoes the exact amount.
Test testGiveXPDirectAwardScriptAndWire()
	ResetWorld()
	Local A.ActorInstance = MakeCombatant(7, 1000, 8)
	GiveXP(A, 250)
	Assert(A\XP = 250)
	Assert(Cap_LastScript$ = "LevelUp")
	Assert(Cap_XPCount = 1)
	Assert(RCE_IntFromStr(Cap_XPData$) = 250)
	ResetWorld()
End Test

; XP awarded to a pet is redirected ENTIRELY to its leader; the pet keeps
; none.
Test testGiveXPRedirectsToLeader()
	ResetWorld()
	Local Owner.ActorInstance = MakeCombatant(7, 1000, 8)
	Local Pet.ActorInstance = MakeCombatant(-1, 1000, 8)
	Pet\Leader = Owner
	GiveXP(Pet, 100)
	Assert(Owner\XP = 100)
	Assert(Pet\XP = 0)
	ResetWorld()
End Test

; ============================================================================
; KillActor
; ============================================================================

; Killing a faction member costs the killer CombatRatingAdjust rating with
; that faction, and awards (VictimLevel - KillerLevel) * XPMultiplier +
; Rand(0,20) XP. The wire XP echo matches the awarded amount exactly.
Test testKillActorFactionDropAndXPAward()
	ResetWorld()
	SeedRnd(7001)
	CombatRatingAdjust = 10
	Local Killer.ActorInstance = MakeCombatant(9, 1000, 8)
	Killer\Level = 5
	Local Victim.ActorInstance = MakeCombatant(-1, 0, 8)
	Victim\Level = 9
	Victim\HomeFaction = 4
	Victim\Actor\XPMultiplier = 10
	Killer\FactionRatings[4] = 50
	KillActor(Victim, Killer)
	Assert(Killer\FactionRatings[4] = 40)
	; Diff = 9 - 5 = 4; XP = 4 * 10 + Rand(0, 20) = 40..60
	Assert(Killer\XP >= 40)
	Assert(Killer\XP <= 60)
	Assert(RCE_IntFromStr(Cap_XPData$) = Killer\XP)
	Assert(Cap_LastScript$ = "LevelUp")
	Assert(Cap_FreeCount = 1)   ; NPC victim is freed
	ResetWorld()
End Test

; Faction rating floors at 0 rather than going negative.
Test testKillActorFactionRatingFloorsAtZero()
	ResetWorld()
	SeedRnd(7002)
	CombatRatingAdjust = 10
	Local Killer.ActorInstance = MakeCombatant(9, 1000, 8)
	Local Victim.ActorInstance = MakeCombatant(-1, 0, 8)
	Victim\HomeFaction = 4
	Killer\FactionRatings[4] = 5
	KillActor(Victim, Killer)
	Assert(Killer\FactionRatings[4] = 0)
	ResetWorld()
End Test

; Killing a victim at or below the killer's level still pays the
; level-difference floor of 1: XP = 1 * XPMultiplier + Rand(0,20).
Test testKillActorLevelDiffFloorsAtOne()
	ResetWorld()
	SeedRnd(7003)
	Local Killer.ActorInstance = MakeCombatant(9, 1000, 8)
	Killer\Level = 50
	Local Victim.ActorInstance = MakeCombatant(-1, 0, 8)
	Victim\Level = 1
	Victim\Actor\XPMultiplier = 10
	KillActor(Victim, Killer)
	Assert(Killer\XP >= 10)
	Assert(Killer\XP <= 30)
	ResetWorld()
End Test

; A kill with no killer (environment death) awards nothing and adjusts no
; faction state.
Test testKillActorWithNullKillerAwardsNothing()
	ResetWorld()
	Local Victim.ActorInstance = MakeCombatant(-1, 0, 8)
	KillActor(Victim, Null)
	Assert(Cap_XPCount = 0)
	Assert(Cap_FreeCount = 1)
	ResetWorld()
End Test

; ============================================================================
; GetArmourLevel (real Inventories.bb math, used by every damage formula)
; ============================================================================

; Sums ArmourLevel over the Shield..Feet slots only, skipping broken
; (0-health) pieces, non-armour items, and armour parked outside the
; equipped-armour slot range (e.g. a ring slot).
Test testGetArmourLevelSumsOnlyLiveEquippedArmour()
	ResetWorld()
	Local A.ActorInstance = MakeCombatant(0, 100, 8)
	GiveArmour(A, 3, SlotI_Chest)
	GiveArmour(A, 5, SlotI_Legs)
	Local Broken.ItemInstance = GiveArmour(A, 7, SlotI_Hat)
	Broken\ItemHealth = 0
	GiveArmour(A, 9, SlotI_Ring1)       ; outside SlotI_Shield..SlotI_Feet
	GiveWeapon(A, W_OneHand, 20, 2)     ; weapon slot not in the armour loop
	Assert(GetArmourLevel(A\Inventory) = 8)
	ResetWorld()
End Test
