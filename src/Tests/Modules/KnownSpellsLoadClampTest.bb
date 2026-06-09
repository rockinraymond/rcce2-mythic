; Tests for the KnownSpells[] sanitisation in ReadActorInstance
; (Modules/Actors.bb). The character load path reads 1000 (KnownSpells,
; SpellLevels) pairs off disk via ReadShort and, as of the KnownSpells
; load-clamp fix, zeroes any slot whose spell ID falls outside the valid
; SpellsList index range before storing it.
;
; Why this matters: KnownSpells[i] is used DIRECTLY as a SpellsList(...)
; index. SpellsList is Dim'd (65534) -> valid indices 0..65534. ReadShort
; returns a value a corrupt or tampered Accounts.dat can push out of that
; range -- a negative ID, or (if ReadShort reads unsigned) a value like
; 65535. BlitzForge emits array-bounds checks only in debug builds; the
; shipped release Server.exe does NOT, so SpellsList(OOB) reads garbage
; memory, casts it to a Spell handle, and crashes the shared server
; process when P_FetchCharacter (ServerNet.bb) dereferences it on
; character-list send. The `If Sp <> Null` guard there runs AFTER the OOB
; read, so it cannot save us -- the clamp has to happen at the load
; boundary. The sibling MemorisedSpells field two lines below already
; clamps for exactly this OOB reason; KnownSpells was the missed slot.
;
; What this pins: the load-site clamp zeroes BOTH the id and its paired
; level for an out-of-range slot (making it inert under every
; `SpellLevels[i] > 0` gate), while a legitimate saved (id, level) pair
; survives unchanged. The two-sided test (< 0 Or > 65534) neutralises the
; OOB regardless of whether ReadShort is signed or unsigned -- the
; ReadShort-round-trip cases below assert that directly.
;
; Actors.bb itself can't be Included into a test build (it pulls in the
; Items / world graph). Per the established pattern (ActorLoadFloatClampTest.bb,
; ClampFloatTest.bb), replicate the load-site logic verbatim and the
; ReadShort round-trip (PeekShort after a PokeShort = ReadShort after a
; WriteShort against the save stream). A refactor that changes the clamp
; range must update the production copy and this duplicate -- the trigger
; to refresh this test.
;
; NOT Strict: matches the non-Strict production module and ActorLoadFloatClampTest.bb.

Const SpellsListMaxIndex = 65534

; Replicated load-site clamp from Actors.bb ReadActorInstance. Production
; applies this inline to A\KnownSpells[i] / A\SpellLevels[i]; here it writes
; the clamped result into globals (Blitz can't return two values). True if
; the slot was already valid, False if it was zeroed.
Global ClampedId%, ClampedLevel%
Function ClampKnownSpellSlot%(rawId, rawLevel)
	ClampedId = rawId
	ClampedLevel = rawLevel
	If ClampedId < 0 Or ClampedId > SpellsListMaxIndex
		ClampedId = 0
		ClampedLevel = 0
		Return False
	EndIf
	Return True
End Function

; Round-trip a short through a 2-byte Bank exactly as WriteShort/ReadShort
; do against the save stream, returning the read-back value. A negative
; saved ID survives this round-trip as an out-of-range value (either signed
; -N or its unsigned read-back), which is precisely why the clamp has to
; happen AFTER the read.
Function RoundTripShort%(v)
	Local b = CreateBank(2)
	PokeShort(b, 0, v)
	Local out = PeekShort(b, 0)
	FreeBank(b)
	Return out
End Function


; A legitimate (id, level) pair inside the valid range passes through the
; clamp unchanged -- the fix must not disturb real spellbooks.
Test testValidSlotPassesThrough()
	Assert(ClampKnownSpellSlot(1, 4) = True)
	Assert(ClampedId = 1)
	Assert(ClampedLevel = 4)

	Assert(ClampKnownSpellSlot(500, 7) = True)
	Assert(ClampedId = 500)
	Assert(ClampedLevel = 7)

	; The maximum valid SpellsList index is accepted (inclusive boundary).
	Assert(ClampKnownSpellSlot(SpellsListMaxIndex, 1) = True)
	Assert(ClampedId = SpellsListMaxIndex)
	Assert(ClampedLevel = 1)

	; The largest value a SIGNED ReadShort can produce (32767) is still in
	; range, so it passes -- only negatives / >65534 are OOB.
	Assert(ClampKnownSpellSlot(32767, 9) = True)
	Assert(ClampedId = 32767)
	Assert(ClampedLevel = 9)
End Test

; An already-empty slot (id 0, level 0) is valid and untouched.
Test testEmptySlotUnchanged()
	Assert(ClampKnownSpellSlot(0, 0) = True)
	Assert(ClampedId = 0)
	Assert(ClampedLevel = 0)
End Test

; A negative spell ID (the core OOB case) zeroes the slot -- both the id and
; its paired level -- so it is skipped by every SpellLevels>0 gate and never
; reaches SpellsList(...).
Test testNegativeIdZeroesSlot()
	Assert(ClampKnownSpellSlot(-1, 3) = False)
	Assert(ClampedId = 0)
	Assert(ClampedLevel = 0)

	; The most-negative ReadShort value.
	Assert(ClampKnownSpellSlot(-32768, 5) = False)
	Assert(ClampedId = 0)
	Assert(ClampedLevel = 0)
End Test

; A value above the SpellsList ceiling (e.g. an unsigned read-back of a
; negative, or a future wider field) also zeroes the slot.
Test testAboveCeilingZeroesSlot()
	Assert(ClampKnownSpellSlot(65535, 2) = False)
	Assert(ClampedId = 0)
	Assert(ClampedLevel = 0)

	Assert(ClampKnownSpellSlot(70000, 1) = False)
	Assert(ClampedId = 0)
	Assert(ClampedLevel = 0)
End Test

; Load-contract: a poisoned ID written to disk survives the WriteShort/
; ReadShort round-trip as an out-of-range value, and the load-site clamp
; rejects whatever comes back -- proving the threat is real post-
; serialisation and the clamp neutralises it regardless of ReadShort's
; signedness. A valid ID survives the same round-trip and stays valid.
Test testLoadedNegativeIdIsRejected()
	; -1 round-trips to either -1 (signed) or 65535 (unsigned); both are OOB.
	Local loadedBad = RoundTripShort(-1)
	Assert(ClampKnownSpellSlot(loadedBad, 6) = False)
	Assert(ClampedId = 0)
	Assert(ClampedLevel = 0)

	; A legitimate ID survives the round-trip intact and passes the clamp.
	Local loadedGood = RoundTripShort(42)
	Assert(loadedGood = 42)
	Assert(ClampKnownSpellSlot(loadedGood, 8) = True)
	Assert(ClampedId = 42)
	Assert(ClampedLevel = 8)
End Test
