; Regression tests for ReadItemInstance / WriteItemInstance in
; Modules/Items.bb -- the STREAM (binary file) item-instance codec used by
; the character / inventory save-load path. These are distinct from the
; STRING codec (ItemInstanceToString$ / ItemInstanceFromString) already
; pinned by ItemsTest.bb: the stream pair is what ReadActorInstance and the
; area-ownership loader call against a real save file on disk.
;
; What this pins (the crash-critical loader guards):
;
;   1. The 65535 SENTINEL contract. ItemList is Dim(65534) -> valid indices
;      0..65534, so 65535 is the one value that is BOTH the "no item here"
;      sentinel WriteItemInstance emits for a Null instance AND the first
;      out-of-bounds ItemList index. The safety rests on ReadShort being
;      UNSIGNED: a saved 65535 must read back as 65535 (not -1), so the
;      `If ID = 65535 Then Return` guard fires and the function returns Null
;      BEFORE indexing ItemList. testSentinel* pins both halves (unsigned
;      read-back + Null return + exact 2-byte consumption).
;
;   2. The UNKNOWN-ID soft-fail + byte-accounting. A non-sentinel ID whose
;      ItemList slot is Null (deleted item, stale character payload) must
;      log + return Null while still CONSUMING the trailing 40 attribute
;      shorts + 1 health byte, so the next record in the stream stays
;      aligned. A trailing marker byte proves the exact 83-byte consumption.
;
;   3. The EOF GUARD on that consume loop. If the unknown-ID record is
;      truncated (file ends mid-record), the `If Eof(Stream) Then Exit`
;      guard must stop the consume loop rather than spin reading past EOF.
;      Returns Null, no crash, no overrun.
;
;   4. The happy-path round trip: a valid instance survives
;      WriteItemInstance -> ReadItemInstance byte-for-byte (Item ref,
;      ItemHealth, all 40 attributes), consuming exactly its 83 bytes.
;
; NOT Strict (matches ReadBoundedStringTest.bb / ActorLoadFloatClampTest.bb):
; WriteItemInstance / ReadItemInstance take their Stream parameter untyped
; (defaults to Int), which Strict cannot auto-convert from the BBStream that
; ReadFile / WriteFile return. A non-Strict test passes the handle cleanly,
; exactly as the production callers (non-Strict Items.bb) do.

; --- External type stubs (mirrors ItemsTest.bb) ---------------------------
; Items.bb references Attributes (defined in Actors.bb) and ActorInstance.
; Stub both inline so we don't drag Actors.bb (with its network/world deps)
; into this unit-test build.
Type Attributes
	Field Value[39]
	Field Maximum[39]
	Field My_ID
End Type

Type ActorInstance
	Field Account
End Type

; --- RCE wire-format helpers (mirrors ItemsTest.bb) -----------------------
; Items.bb's STRING codec uses these; they must resolve for the include to
; link even though the tests here exercise only the STREAM codec.
Global ReadItemInstanceTest_ConvertBank.BBBank = CreateBank(8)

Function RCE_IntFromStr(Dat$)
	PokeInt ReadItemInstanceTest_ConvertBank, 0, 0
	For i = 1 To Len(Dat$)
		PokeByte ReadItemInstanceTest_ConvertBank, i - 1, Asc(Mid$(Dat$, i, 1))
	Next
	Return PeekInt(ReadItemInstanceTest_ConvertBank, 0)
End Function

Function RCE_StrFromInt$(Num, Length = 4)
	PokeInt ReadItemInstanceTest_ConvertBank, 0, Num
	Dat$ = ""
	For i = Length - 1 To 0 Step -1
		Dat$ = Chr$(PeekByte(ReadItemInstanceTest_ConvertBank, i)) + Dat$
	Next
	Return Dat$
End Function

; --- Logging stub ---------------------------------------------------------
Global MainLog = 0

Function WriteLog(LogID%, Message$, Timestamp% = True, Datestamp% = False)
End Function

; --- SafeWrite stubs (SaveItems routes through these) ---------------------
Function SafeWriteOpen$(FinalPath$)
	Return FinalPath$
End Function

Function SafeWriteCommit%(TempPath$, FinalPath$, F)
	Return True
End Function

; --- Language + bounded-string stubs --------------------------------------
Function LanguageString$(key$)
	Return key
End Function

Function ReadBoundedString$(F, MaxLen)
	Return ""
End Function

Include "Modules\Items.bb"

; --- Test fixtures --------------------------------------------------------
Global testPath$ = CurrentDir$() + "readiteminstance_test.dat"

Function Cleanup()
	If FileType(testPath$) = 1 Then DeleteFile(testPath$)
End Function

; Reset module state between tests: remove every Item / ItemInstance /
; Attributes object so CreateItem starts from a clean ItemList walk and no
; objects leak across tests. Mirrors ItemsTest.bb's ClearItemList.
Function ClearItemList()
	Delete Each ItemInstance
	Delete Each Item
	Delete Each Attributes
End Function

; ==========================================================================
; 1. Sentinel: a Null instance round-trips to Null, consuming exactly 2 bytes
; ==========================================================================

; WriteItemInstance(Null) emits WriteShort 65535. ReadItemInstance must read
; that back (unsigned) as the sentinel, return Null, and leave the stream
; positioned exactly after the 2-byte short -- proven by a trailing marker.
Test testSentinelInstanceRoundTripsToNullAndConsumesTwoBytes()
	ClearItemList()
	Cleanup()

	F = WriteFile(testPath$)
	WriteItemInstance(F, Null)   ; writes the 65535 sentinel short
	WriteByte F, 123             ; marker immediately after
	CloseFile F

	F = ReadFile(testPath$)
	restored.ItemInstance = ReadItemInstance(F)
	Assert(restored = Null)
	; Only the 2-byte sentinel was consumed -> marker is the next byte.
	marker = ReadByte(F)
	Assert(marker = 123)
	CloseFile F

	Cleanup()
	ClearItemList()
End Test

; Direct invariant: the 65535 sentinel depends on ReadShort being UNSIGNED.
; A saved 65535 must read back as 65535, NOT -1 -- otherwise the
; `If ID = 65535 Then Return` guard would miss and the value (or a negative)
; would fall through to ItemList(ID) and index out of bounds.
Test testReadShortIsUnsignedSoSentinelSurvives()
	Cleanup()

	F = WriteFile(testPath$)
	WriteShort F, 65535
	CloseFile F

	F = ReadFile(testPath$)
	v = ReadShort(F)
	CloseFile F
	Assert(v = 65535)

	Cleanup()
End Test

; ==========================================================================
; 2. Happy path: a valid instance survives the stream round trip
; ==========================================================================

Test testValidInstanceStreamRoundTrip()
	ClearItemList()
	Cleanup()

	sword.Item = CreateItem()
	sword\Name$ = "Sword"

	original.ItemInstance = CreateItemInstance(sword)
	original\ItemHealth = 75
	For idx = 0 To 39
		original\Attributes\Value[idx] = idx - 20  ; mix of negative + positive
	Next

	F = WriteFile(testPath$)
	WriteItemInstance(F, original)
	WriteByte F, 99   ; marker -- proves exactly 83 bytes were written/consumed
	CloseFile F

	F = ReadFile(testPath$)
	restored.ItemInstance = ReadItemInstance(F)
	Assert(restored <> Null)
	Assert(ItemInstancesIdentical(original, restored) = True)
	marker = ReadByte(F)
	Assert(marker = 99)
	CloseFile F

	Cleanup()
	ClearItemList()
End Test

; ==========================================================================
; 3. Unknown ID: soft-fail to Null but consume the full 83-byte record
; ==========================================================================

; ItemList(9999) is Null (no item ever created with that ID in these tests).
; ReadItemInstance must log + return Null while still consuming the 40
; attribute shorts + 1 health byte so the following record stays aligned.
; The trailing marker pins the exact byte accounting.
Test testUnknownIdReturnsNullAndConsumesFullRecord()
	ClearItemList()
	Cleanup()

	F = WriteFile(testPath$)
	WriteShort F, 9999          ; ID with an empty ItemList slot
	For j = 0 To 39
		WriteShort F, 5000      ; 40 attribute shorts
	Next
	WriteByte F, 100            ; health byte
	WriteByte F, 77             ; marker immediately after the record
	CloseFile F

	F = ReadFile(testPath$)
	restored.ItemInstance = ReadItemInstance(F)
	Assert(restored = Null)
	marker = ReadByte(F)
	Assert(marker = 77)         ; record consumed exactly -> marker intact
	CloseFile F

	Cleanup()
	ClearItemList()
End Test

; ==========================================================================
; 4. Unknown ID + truncated record: the EOF guard stops the consume loop
; ==========================================================================

; A truncated record (unknown ID, then the file ends before the attribute
; payload) must not spin the consume loop past EOF. The
; `If Eof(Stream) Then Exit` guard returns Null cleanly.
Test testUnknownIdTruncatedRecordHitsEofGuard()
	ClearItemList()
	Cleanup()

	F = WriteFile(testPath$)
	WriteShort F, 9999   ; unknown ID, then immediate EOF -- no attribute bytes
	CloseFile F

	F = ReadFile(testPath$)
	restored.ItemInstance = ReadItemInstance(F)
	Assert(restored = Null)
	Assert(Eof(F) = True)   ; consumed to end, no overrun / no hang
	CloseFile F

	Cleanup()
	ClearItemList()
End Test
