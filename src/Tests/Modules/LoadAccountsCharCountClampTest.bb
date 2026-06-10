; Pins the per-account character-count cap in LoadAccounts
; (src/Modules/AccountsServer.bb ~336-339).
;
; THE GUARD:
;   Chars = ReadByte(F)
;   If Chars > 10 Then Chars = 10
;   For i = 1 To Chars
;       A\Character[i - 1] = ReadActorInstance(F)   ; Character is [9] -> 0..9
;       ...
;
; WHY IT MATTERS: Chars is read as a single unsigned byte (0..255) from
; Accounts.dat at server boot. It then drives a `For i = 1 To Chars` loop
; that (a) indexes A\Character[i - 1] / A\QuestLog[i - 1] / A\ActionBar[i - 1],
; each a Field[9] (valid 0..9), and (b) reads a full character blob
; (ActorInstance + 500 QuestLog entries + 36 ActionBar slots) per iteration.
; A corrupted or hand-edited Accounts.dat with 255 in this slot would, without
; the cap, both Field-OOB at i - 1 = 10..254 AND walk hundreds of records
; past EOF -- crashing the shared server on every startup until the file is
; manually repaired. The cap pins Chars into 1..10 so the loop stays inside
; the Character[9] bound. (The byte is unsigned, so there is no negative
; case to guard; the per-iteration `If Eof(F) Then Exit` guards inside the
; QuestLog / ActionBar loops handle a file that ends mid-character.)
;
; SCOPE / why this is the slice tested here:
;   - The STRING-length + EOF guards LoadAccounts leans on (every User /
;     Pass / Email / Ignore / QuestLog / ActionBar field is read through
;     ReadBoundedString$) are already pinned directly against the real
;     Logging.bb in ReadBoundedStringTest.bb (length-cap, negative-length,
;     zero-length, stop-at-EOF, null-handle). That is the truncated-tail
;     string-bounding contract.
;   - The remaining LoadAccounts-specific structural guards (the
;     `While Eof(F) = False` outer truncated-tail tolerance, the per-character
;     `If Eof Then Exit` exits, and the Null-ReadActorInstance handling)
;     can only be pinned end-to-end, and LoadAccounts opens a HARDCODED path
;     ("Data\Server Data\Accounts.dat") it does not accept as a parameter.
;     Exercising it would require creating that nested directory tree under
;     the test working directory and writing a crafted save there -- a
;     filesystem-fragile pattern with no precedent in this suite (every other
;     file-I/O test uses a flat CurrentDir$() temp file) and a clobber/leak
;     risk if the process crashes mid-test. Per the project's
;     replicated-logic convention for un-loadable loaders
;     (SpawnWaypointClampTest / KnownSpellsLoadClampTest / etc.), this file
;     pins the self-contained count-cap expression; the structural slice is
;     intentionally left to ReadBoundedStringTest's EOF coverage rather than
;     forced into a brittle test.
;
; REPLICATED-LOGIC test: AccountsServer.bb's LoadAccounts cannot be unit-run
; without the hardcoded save path + the Accounts window/global setup, so
; clampCharCount% mirrors the source expression verbatim. A change to the
; production cap (or the Character[9] bound it protects) must update this
; duplicate -- the trigger to refresh the test.
;
; NOT Strict: matches the non-Strict production module and the sibling
; clamp tests.

; Character / QuestLog / ActionBar are each Field[9] -> 10 valid slots.
Const MaxCharacters = 10

; Exact mirror of the cap applied at LoadAccounts after `Chars = ReadByte(F)`.
Function clampCharCount%(Chars%)
	If Chars > MaxCharacters Then Return MaxCharacters
	Return Chars
End Function

; Round-trip a value through a 1-byte Bank exactly as WriteByte/ReadByte do
; against the save stream, returning the read-back value. ReadByte is
; unsigned (0..255), so a corrupt high byte reads back as a large positive
; count -- which is exactly what the cap has to defend against.
Function RoundTripByte%(v)
	Local b = CreateBank(1)
	PokeByte(b, 0, v)
	Local out = PeekByte(b, 0)
	FreeBank(b)
	Return out
End Function


; Counts within the valid range pass through untouched, including the
; inclusive upper bound (10 -> indexes Character[0..9]).
Test testValidCharCountsPassThrough()
	Assert(clampCharCount%(0) = 0)
	Assert(clampCharCount%(1) = 1)
	Assert(clampCharCount%(5) = 5)
	Assert(clampCharCount%(MaxCharacters) = MaxCharacters)
End Test

; The first over-bound value caps to 10. 11 would make i - 1 reach 10, one
; past the Character[9] bound.
Test testJustAboveBoundCapsToTen()
	Assert(clampCharCount%(11) = MaxCharacters)
End Test

; The corrupt-file headline case: a 255 byte caps to 10 instead of reading
; 255 character blobs past EOF.
Test testMaxByteCapsToTen()
	Assert(clampCharCount%(255) = MaxCharacters)
End Test

; Load-contract: a corrupt count byte survives the WriteByte/ReadByte
; round-trip as a large unsigned value, and the cap pins it to 10. A
; legitimate count survives the same round-trip and passes through.
Test testLoadedCorruptCountIsCapped()
	; 255 (0xFF) round-trips unsigned to 255; capped to 10.
	Local loadedBad = RoundTripByte(255)
	Assert(loadedBad = 255)
	Assert(clampCharCount%(loadedBad) = MaxCharacters)

	; A legitimate 3-character account survives intact and passes the cap.
	Local loadedGood = RoundTripByte(3)
	Assert(loadedGood = 3)
	Assert(clampCharCount%(loadedGood) = 3)
End Test
