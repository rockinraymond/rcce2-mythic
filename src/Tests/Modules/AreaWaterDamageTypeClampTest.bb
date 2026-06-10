; Pins the water DamageType bounds-clamp applied in ServerLoadArea
; (src/Modules/ServerAreas.bb ~366, in the `For i = 1 To Waters` block,
; right after `W\DamageType = ReadShort(F)`).
;
; This is the SIBLING clamp to the one SpawnWaypointClampTest.bb pins. Both
; live in ServerLoadArea; SpawnWaypoint guards a Field[1999] waypoint index,
; this one guards the water-damage element index. They are separate guards
; on separate fields and either can regress independently, so they get
; separate pins.
;
; WHY THIS MATTERS: W\DamageType is read SIGNED via ReadShort (-32768..32767)
; from the area .dat. It is later used DIRECTLY to index A\Resistances[19]
; (a Field[19], valid 0..19) in the runtime SafeZone-damage loop
; (GameServer.bb ~773), and DamageTypes$ is Dim'd (19) as well. BlitzForge
; emits array-bounds checks only in debug builds; the shipped release
; Server.exe does NOT, so A\Resistances[OOB] reads/writes adjacent memory and
; crashes -- or silently corrupts -- the shared server process, taking every
; connected player down. A corrupt or hand-edited area file with an
; out-of-range DamageType is the trigger.
;
; The fix clamps at the load boundary: `If v < 0 Or v > 19 Then v = 0`.
; Slot 0 is the valid default damage type, matching the SpawnWaypoint clamp's
; choice of slot 0 as the safe fallback.
;
; This is a REPLICATED-LOGIC test, exactly per SpawnWaypointClampTest.bb /
; ActorLoadFloatClampTest.bb / KnownSpellsLoadClampTest.bb: ServerLoadArea
; cannot be unit-loaded without the full area-file binary format + the
; ServerAreas dependency graph, and (unlike a tiny inline expression) its
; multi-thousand-field read sequence is not safely reproducible in a test.
; clampWaterDamageType% below mirrors the source expression verbatim; the
; source fix is additionally verified by clean compile + the traced consumer
; chain (GameServer.bb A\Resistances[SW\DamageType]). The contract-bearing
; assertions are the NEGATIVE and >19 cases -- a `> 19`-only guard (dropping
; the `< 0` half) would let a sign-bit-set ReadShort through and Field-OOB
; with a negative index. This test fails if that half is removed.
;
; NOT Strict: matches the non-Strict production module and the sibling
; SpawnWaypointClampTest.bb.

Const ResistanceMaxIndex = 19

; Exact mirror of the clamp applied at ServerLoadArea's water-load loop.
Function clampWaterDamageType%(v%)
	If v < 0 Or v > ResistanceMaxIndex Then Return 0
	Return v
End Function

; Round-trip a short through a 2-byte Bank exactly as WriteShort/ReadShort do
; against the save stream, returning the read-back value. A negative saved
; DamageType survives this round-trip as an out-of-range value, which is
; precisely why the clamp has to happen AFTER the read.
Function RoundTripShort%(v)
	Local b = CreateBank(2)
	PokeShort(b, 0, v)
	Local out = PeekShort(b, 0)
	FreeBank(b)
	Return out
End Function


; In-range values pass through untouched (lower edge, interior, upper edge).
; Field[19] is valid 0..19 inclusive.
Test testInRangeDamageTypesUnchangedAtBothEdges()
	Assert(clampWaterDamageType%(0) = 0)
	Assert(clampWaterDamageType%(1) = 1)
	Assert(clampWaterDamageType%(10) = 10)
	Assert(clampWaterDamageType%(ResistanceMaxIndex) = ResistanceMaxIndex)
End Test

; Just past the upper bound clamps to the default slot (0). 20 is the first
; out-of-bounds index for a Field[19] (slots 0..19 inclusive).
Test testJustAboveUpperBoundClampsToZero()
	Assert(clampWaterDamageType%(20) = 0)
End Test

; The full positive reach of a signed ReadShort clamps. This is the value a
; corrupt .dat can carry in the high byte.
Test testMaxSignedShortClampsToZero()
	Assert(clampWaterDamageType%(32767) = 0)
End Test

; The NEGATIVE half -- the case a `> 19`-only guard would miss. ReadShort
; yields negatives for any value with the sign bit set; an unguarded negative
; index Field-OOBs into memory before A\Resistances[0].
Test testNegativeDamageTypesClampToZero()
	Assert(clampWaterDamageType%(-1) = 0)
	Assert(clampWaterDamageType%(-100) = 0)
	Assert(clampWaterDamageType%(-32768) = 0)
End Test

; Load-contract: a poisoned DamageType written to disk survives the
; WriteShort/ReadShort round-trip as an out-of-range value, and the load-site
; clamp rejects whatever comes back -- regardless of ReadShort's signedness.
; A legitimate value survives the same round-trip and stays valid.
Test testLoadedOutOfRangeDamageTypeIsClamped()
	; -1 round-trips to either -1 (signed) or 65535 (unsigned); both OOB.
	Local loadedBad = RoundTripShort(-1)
	Assert(clampWaterDamageType%(loadedBad) = 0)

	; A value past the ceiling survives the round-trip and is clamped.
	Assert(clampWaterDamageType%(RoundTripShort(500)) = 0)

	; A legitimate damage-type index survives intact and passes the clamp.
	Local loadedGood = RoundTripShort(7)
	Assert(loadedGood = 7)
	Assert(clampWaterDamageType%(loadedGood) = 7)
End Test
