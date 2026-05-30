; Tests for SafeWriteOpen$ / SafeWriteCommit% in Modules/Logging.bb.
;
; These helpers are the engine + tools' atomic-write primitive: write to a
; .tmp, then promote it to the final path only on success, demoting the
; previous file to .bak. Every persistent on-disk save is supposed to route
; through them (CLAUDE.md "Atomic writes"), and as of PR #449/#452-era work
; the three editor-tool primary save paths (RC Architect / Tree / Caves) now
; do too. Despite being load-bearing across the whole codebase, the helper
; had ZERO test coverage -- this pins its contract directly.
;
; Unlike most tests in this dir, this is a genuine INTEGRATION test: it
; Includes the real Logging.bb and exercises the actual helpers against real
; files in the current directory (Logging.bb has no RakNet/world deps, so it
; CAN be Included offline -- same precedent as ReadBoundedStringTest.bb).
;
; NOT Strict: SafeWriteCommit% takes its file-handle param untyped (Int),
; which Strict can't auto-convert from the BBStream that WriteFile returns --
; same reason ReadBoundedStringTest.bb is non-Strict.

; LogMode=0 / MainLog=0 keep Logging.bb's WriteLog calls silent no-ops.
Global LogMode = 0
Global MainLog = 0

Include "Modules\Logging.bb"

Global swFinal$ = CurrentDir$() + "safewrite_commit_test.dat"
Global swTemp$  = swFinal$ + ".tmp"
Global swBak$   = swFinal$ + ".bak"

Function SWCleanup()
	If FileType(swFinal$) = 1 Then DeleteFile(swFinal$)
	If FileType(swTemp$) = 1 Then DeleteFile(swTemp$)
	If FileType(swBak$) = 1 Then DeleteFile(swBak$)
End Function

; Write `n` bytes (value 65 = 'A') to a fresh file at `path`.
Function SeedFile(path$, n)
	f = WriteFile(path$)
	For i = 1 To n
		WriteByte f, 65
	Next
	CloseFile f
End Function


; SafeWriteOpen$ derives the temp path by appending ".tmp".
Test testOpenAppendsTmpSuffix()
	Assert(SafeWriteOpen$("foo.dat") = "foo.dat.tmp")
	Assert(SafeWriteOpen$("a\b\c.lgt") = "a\b\c.lgt.tmp")
End Test

; A non-empty temp is promoted to the final path; the temp is removed and the
; written content survives the promotion.
Test testCommitPromotesNonEmptyTemp()
	SWCleanup()
	tmp$ = SafeWriteOpen$(swFinal$)
	f = WriteFile(tmp$)
	WriteInt f, 12345
	ok = SafeWriteCommit%(tmp$, swFinal$, f)
	Assert(ok = True)
	Assert(FileType(swFinal$) = 1)   ; final exists
	Assert(FileType(swTemp$) <> 1)   ; temp removed
	rf = ReadFile(swFinal$)
	Assert(ReadInt(rf) = 12345)      ; content intact through promote
	CloseFile rf
	SWCleanup()
End Test

; A zero-length temp is REFUSED -- a pre-existing final must survive intact
; (this is the data-loss protection the tool save paths now rely on).
Test testCommitRefusesEmptyTempAndKeepsOriginal()
	SWCleanup()
	SeedFile(swFinal$, 4)            ; pre-existing 4-byte save
	tmp$ = SafeWriteOpen$(swFinal$)
	f = WriteFile(tmp$)              ; open temp, write NOTHING (0 bytes)
	ok = SafeWriteCommit%(tmp$, swFinal$, f)
	Assert(ok = False)               ; refused to promote empty
	Assert(FileType(swFinal$) = 1)   ; original still present
	Assert(FileSize(swFinal$) = 4)   ; original content untouched
	Assert(FileType(swTemp$) <> 1)   ; empty temp cleaned up
	SWCleanup()
End Test

; A successful commit over an existing file demotes the old version to .bak.
Test testCommitDemotesExistingToBak()
	SWCleanup()
	SeedFile(swFinal$, 7)            ; old final, 7 bytes
	tmp$ = SafeWriteOpen$(swFinal$)
	f = WriteFile(tmp$)
	WriteByte f, 66
	WriteByte f, 66                 ; new final, 2 bytes
	ok = SafeWriteCommit%(tmp$, swFinal$, f)
	Assert(ok = True)
	Assert(FileSize(swFinal$) = 2)   ; new content promoted
	Assert(FileType(swBak$) = 1)     ; old demoted to .bak
	Assert(FileSize(swBak$) = 7)     ; .bak holds the previous content
	SWCleanup()
End Test

; If the temp was never created (e.g. WriteFile returned 0), commit refuses
; and the original file is untouched -- no clobber on a failed open.
Test testCommitWithMissingTempRefuses()
	SWCleanup()
	SeedFile(swFinal$, 5)
	ok = SafeWriteCommit%(swTemp$, swFinal$, 0)   ; temp never written, handle 0
	Assert(ok = False)
	Assert(FileType(swFinal$) = 1)
	Assert(FileSize(swFinal$) = 5)
	SWCleanup()
End Test
