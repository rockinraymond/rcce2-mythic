Strict
EnableGC

; Regression tests pinning BVM_ScriptPathIsSafe% -- the path-traversal floor
; that keeps a non-privileged NPC script from escaping the RCScriptFiles$
; sandbox via the script file-I/O BVMs (ScriptingCommands.bb:1913-1926).
;
; Why this guard is load-bearing: of the eight FS BVMs that route through it,
; three -- BVM_READFILE, BVM_FILESIZE, BVM_FILETYPE -- DELIBERATELY run with
; no BVM_RequirePrivileged() gate (read/stat are treated as non-mutating, so
; any NPC's RightClick/Examine/ItemScript can call them). For those three,
; BVM_ScriptPathIsSafe is the ONLY barrier between a hostile content script
; and arbitrary host-file read/probe (e.g. ..\..\Server Data\Privileged
; Scripts.dat, the running .exe, system configs). The other five FS BVMs
; (DELETEFILE / WRITEFILE / OPENFILE / APPENDFILE / CREATEDIR) layer a
; privilege gate on top, but the path check is the floor for all eight.
; This invariant had zero test coverage; a refactor that "tidies" the ..
; check or swaps Instr for a segment-splitter could silently re-open the
; sandbox with no failing test.
;
; ScriptingCommands.bb can't be Included into a test build -- it pulls in the
; whole actor / item / wire / scripting graph. Following the established
; ClampFloatTest.bb / BVMPrivilegeGateTest.bb pattern, the function body is
; replicated here CHARACTER-IDENTICAL to production (it is a pure string
; predicate with no SI/Object/Handle dependencies, so no scaffolding is
; needed). A refactor that changes the guard must update both copies; that
; is the trigger to refresh this test.
;
; Blitz string-literal notes: Blitz does NOT process backslash escapes, so
; "..\x" is literally dot-dot-backslash-x; embedded Chr$(0) is valid in a
; length-prefixed Blitz string and Len/Asc see it.

; --- Replicated production logic (ScriptingCommands.bb:1913-1926) ----------
Function BVM_ScriptPathIsSafe%(Name$)
	If Name$ = "" Then Return False
	If Instr(Name$, "..") > 0 Then Return False
	If Left$(Name$, 1) = "\" Or Left$(Name$, 1) = "/" Then Return False
	; Drive letter "C:..." or any colon at position 2.
	If Len(Name$) >= 2 And Mid$(Name$, 2, 1) = ":" Then Return False
	; Reject control bytes / non-printable.
	Local i, c
	For i = 1 To Len(Name$)
		c = Asc(Mid$(Name$, i, 1))
		If c < 32 Or c = 127 Then Return False
	Next
	Return True
End Function


; Ordinary in-sandbox names are accepted. A mid-path separator (forward or
; back slash) is allowed -- only a LEADING slash is an absolute-path marker.
; Single dots and spaces are fine; only the ".." sequence is a traversal.
Test testAcceptsOrdinaryNames()
	Assert(BVM_ScriptPathIsSafe("player_log.txt") = True)
	Assert(BVM_ScriptPathIsSafe("a") = True)
	Assert(BVM_ScriptPathIsSafe("my.save") = True)
	Assert(BVM_ScriptPathIsSafe("a.b.c") = True)
	Assert(BVM_ScriptPathIsSafe("file with spaces.txt") = True)
	Assert(BVM_ScriptPathIsSafe("subdir/data.dat") = True)
	Assert(BVM_ScriptPathIsSafe("subdir\data.dat") = True)
End Test

; The empty string is rejected (production's first guard) -- an empty operand
; would resolve to RCScriptFiles$ itself.
Test testRejectsEmpty()
	Assert(BVM_ScriptPathIsSafe("") = False)
End Test

; The core threat: any ".." sequence is rejected. This includes the bare
; "..", traversal with either separator, and ".." nested mid-path. The guard
; uses a SUBSTRING test (Instr), so "a..b" -- dots with no separator -- is
; also rejected. That is deliberately strict; this test pins it so a future
; switch to segment-aware matching is a conscious, reviewed change.
Test testRejectsTraversal()
	Assert(BVM_ScriptPathIsSafe("..") = False)
	Assert(BVM_ScriptPathIsSafe("../etc/passwd") = False)
	Assert(BVM_ScriptPathIsSafe("..\windows\system32") = False)
	Assert(BVM_ScriptPathIsSafe("sub/../escape") = False)
	Assert(BVM_ScriptPathIsSafe("foo\..\bar") = False)
	Assert(BVM_ScriptPathIsSafe("foo/..") = False)
	; Substring match -- dots without a separator are still rejected.
	Assert(BVM_ScriptPathIsSafe("a..b") = False)
End Test

; A leading slash or backslash marks an absolute path and is rejected,
; otherwise it would escape RCScriptFiles$ to a drive root / UNC path.
Test testRejectsLeadingSlash()
	Assert(BVM_ScriptPathIsSafe("/etc/passwd") = False)
	Assert(BVM_ScriptPathIsSafe("\windows") = False)
	Assert(BVM_ScriptPathIsSafe("\\server\share") = False)
End Test

; A colon at position 2 is a Windows drive letter ("C:\...") or drive-relative
; path ("D:foo") and is rejected.
Test testRejectsDriveLetter()
	Assert(BVM_ScriptPathIsSafe("C:\Windows") = False)
	Assert(BVM_ScriptPathIsSafe("D:foo") = False)
	Assert(BVM_ScriptPathIsSafe("z:") = False)
End Test

; Control bytes (< 32) and DEL (127) are rejected anywhere in the name --
; NUL-splicing and newline/CR injection into a host path are blocked. The
; check scans every byte, so a control byte buried mid-string is caught too.
Test testRejectsControlBytes()
	Assert(BVM_ScriptPathIsSafe(Chr$(0)) = False)
	Assert(BVM_ScriptPathIsSafe("file" + Chr$(0) + ".txt") = False)
	Assert(BVM_ScriptPathIsSafe("line" + Chr$(10) + "two") = False)
	Assert(BVM_ScriptPathIsSafe("cr" + Chr$(13)) = False)
	Assert(BVM_ScriptPathIsSafe(Chr$(31)) = False)
	Assert(BVM_ScriptPathIsSafe("del" + Chr$(127)) = False)
End Test

; A name made only of printable, in-range bytes at the boundary (space = 32,
; '~' = 126) passes the control-byte scan -- confirms the scan boundary is
; the documented one and isn't off-by-one rejecting valid printables.
Test testAcceptsPrintableBoundary()
	Assert(BVM_ScriptPathIsSafe(Chr$(32) + "x") = True)
	Assert(BVM_ScriptPathIsSafe("x" + Chr$(126)) = True)
End Test
