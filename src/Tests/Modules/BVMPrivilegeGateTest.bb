Strict
EnableGC

; Regression tests pinning the BVM privilege-gate contract for the
; "equivalent-effect bypass" cluster in ScriptingCommands.bb.
;
; Seven BVM functions had effects identical to already-gated peers but
; lacked the gate themselves, defeating the privilege model:
;
;   Newly-gated function   Bypass of           Gate chosen
;   -------------------------------------------------------------------
;   BVM_CHANGEGOLD         BVM_SETGOLD         RequirePrivileged
;   BVM_CHANGEMONEY        BVM_SETMONEY        RequirePrivileged
;   BVM_GIVEXP             BVM_SETACTORLEVEL   RequirePrivileged
;   BVM_GIVEKILLXP         BVM_SETACTORLEVEL   RequirePrivileged
;   BVM_SETATTRIBUTE       BVM_KILLACTOR       RequirePrivileged
;   BVM_CHANGEATTRIBUTE    BVM_KILLACTOR       RequirePrivileged
;   BVM_REMOVEZONEINSTANCE (admin-only)        RequirePrivileged
;
; ScriptingCommands.bb can't be Included directly into a test build --
; it pulls in the entire actor / item / wire / scripting graph. Following
; the established ClampFloatTest.bb / ItemsTest.bb pattern, we replicate
; the gate semantics and the gate-relevant prologue of each function,
; then assert the gate behaviour.
;
; Replication shape: production has the helpers read SI fields via
;   SI = Object.ScriptInstance(hSI); If SI = Null Then Return False
;   If SI\Privileged <> 0 Then Return True
; We split SI's fields into individual globals (ScriptActive flag plus
; Privileged / AI / AIContext) so the test can configure script state
; without depending on the Object/Handle table or fighting GC over a
; transient ScriptInstance object. Gate decision logic is verbatim.
; A refactor that changes the gate decision must update both copies.

; --- Replicated gate state --------------------------------------------
; Production: Object.ScriptInstance(hSI). A truthy ScriptActive means
; "the lookup succeeded" -- i.e., there IS an executing script context.
; Setting ScriptActive=False reproduces "hSI is stale / freed".
Global ScriptActive = False
Global ScriptPrivileged = 0
Global ScriptAI = 0
Global ScriptAIContext = 0
Global ScriptName$ = "TestScript"

; Capture the last refusal so tests can assert the gate refusal path
; actually fired (not a silent no-op).
Global LastScriptLog$ = ""

Function BVM_ScriptLog(Message$)
	LastScriptLog$ = Message$
End Function

; Production: Function BVM_RequirePrivileged%() at ScriptingCommands.bb:132
Function BVM_RequirePrivileged%()
	If ScriptActive = False Then Return False
	If ScriptPrivileged <> 0 Then Return True
	BVM_ScriptLog("Privileged BVM call refused from non-privileged script: " + ScriptName)
	Return False
End Function

; Production: Function BVM_RequireSelfOrPrivileged%(Param1%) at ScriptingCommands.bb:148
Function BVM_RequireSelfOrPrivileged%(Param1%)
	If ScriptActive = False Then Return False
	If ScriptPrivileged <> 0 Then Return True
	If Param1% <> 0 And (Param1% = ScriptAI Or Param1% = ScriptAIContext) Then Return True
	BVM_ScriptLog("BVM call refused: target is neither script's actor nor context (" + ScriptName + ")")
	Return False
End Function

; --- Mock-mutation surface ---------------------------------------------
; Each newly-gated function in production performs a state mutation
; (gold transfer, attribute change, zone teardown, ...). Replicate just
; the gate prologue + a single observable side-effect for each, so the
; test asserts the gate actually short-circuits the function body.
Global MutationGold = 0
Global MutationMoney = 0
Global MutationXP = 0
Global MutationKillXP = 0
Global MutationSetAttr = 0
Global MutationChangeAttr = 0
Global MutationRemoveZone = 0

Function MockBVM_CHANGEGOLD(Param1%, Param2%)
	If Not BVM_RequirePrivileged() Then Return
	MutationGold = MutationGold + Param2%
End Function

Function MockBVM_CHANGEMONEY(Param1%, Param2%)
	If Not BVM_RequirePrivileged() Then Return
	MutationMoney = MutationMoney + Param2%
End Function

Function MockBVM_GIVEXP(Param1%, Param2%, Param3%=0)
	If Not BVM_RequirePrivileged() Then Return
	MutationXP = MutationXP + Param2%
End Function

Function MockBVM_GIVEKILLXP(Param1%, Param2%)
	If Not BVM_RequirePrivileged() Then Return
	MutationKillXP = MutationKillXP + 1
End Function

Function MockBVM_SETATTRIBUTE(Param1%, Param2$, Param3%)
	If Not BVM_RequirePrivileged() Then Return
	MutationSetAttr = MutationSetAttr + Param3%
End Function

Function MockBVM_CHANGEATTRIBUTE(Param1%, Param2$, Param3%)
	If Not BVM_RequirePrivileged() Then Return
	MutationChangeAttr = MutationChangeAttr + Param3%
End Function

Function MockBVM_REMOVEZONEINSTANCE(Param1$, Instance%)
	If Not BVM_RequirePrivileged() Then Return
	MutationRemoveZone = MutationRemoveZone + 1
End Function

; --- Test fixture helpers ----------------------------------------------
Function ResetMutationCounters()
	MutationGold = 0
	MutationMoney = 0
	MutationXP = 0
	MutationKillXP = 0
	MutationSetAttr = 0
	MutationChangeAttr = 0
	MutationRemoveZone = 0
	LastScriptLog$ = ""
End Function

Function InstallScript(Privileged%, AI%, AIContext%)
	ScriptActive = True
	ScriptPrivileged = Privileged
	ScriptAI = AI
	ScriptAIContext = AIContext
End Function

Function ClearScript()
	ScriptActive = False
	ScriptPrivileged = 0
	ScriptAI = 0
	ScriptAIContext = 0
End Function

; ======================================================================
; Gate helper contract -- a non-priv script returns False; the refusal
; goes through BVM_ScriptLog so server operators have an audit trail.
; ======================================================================

Test testRequirePrivilegedAllowsPrivilegedScript()
	InstallScript(1, 0, 0)
	Assert(BVM_RequirePrivileged() = True)
End Test

Test testRequirePrivilegedRefusesNonPrivilegedScript()
	InstallScript(0, 0, 0)
	LastScriptLog$ = ""
	Assert(BVM_RequirePrivileged() = False)
	; Refusal must be audit-logged, not silently dropped.
	Assert(Len(LastScriptLog$) > 0)
End Test

Test testRequirePrivilegedFailsClosedOnMissingScript()
	; A stale/invalid script context must not be treated as privileged.
	; Closed-by-default is the entire point of the gate.
	ClearScript()
	Assert(BVM_RequirePrivileged() = False)
End Test

Test testRequireSelfOrPrivilegedAllowsPrivilegedScript()
	InstallScript(1, 0, 0)
	Assert(BVM_RequireSelfOrPrivileged(12345) = True)
End Test

Test testRequireSelfOrPrivilegedAllowsOwnAI()
	InstallScript(0, 777, 0)
	Assert(BVM_RequireSelfOrPrivileged(777) = True)
End Test

Test testRequireSelfOrPrivilegedAllowsOwnContext()
	InstallScript(0, 0, 888)
	Assert(BVM_RequireSelfOrPrivileged(888) = True)
End Test

Test testRequireSelfOrPrivilegedRefusesArbitraryTarget()
	InstallScript(0, 100, 200)
	LastScriptLog$ = ""
	Assert(BVM_RequireSelfOrPrivileged(999) = False)
	Assert(Len(LastScriptLog$) > 0)
End Test

Test testRequireSelfOrPrivilegedRejectsZeroTarget()
	; Param1 = 0 must not match SI\AI = 0 or SI\AIContext = 0 -- otherwise
	; a freshly-created script (both fields default 0) would be allowed
	; to target "actor 0" from any non-priv context.
	InstallScript(0, 0, 0)
	Assert(BVM_RequireSelfOrPrivileged(0) = False)
End Test

Test testRequireSelfOrPrivilegedFailsClosedOnMissingScript()
	ClearScript()
	Assert(BVM_RequireSelfOrPrivileged(0) = False)
	Assert(BVM_RequireSelfOrPrivileged(123) = False)
End Test

; ======================================================================
; Per-function gate enforcement -- the seven newly-gated functions must
; short-circuit when the calling script is non-privileged.
; ======================================================================

Test testChangeGoldGateBlocksNonPrivileged()
	InstallScript(0, 0, 0)
	ResetMutationCounters()
	MockBVM_CHANGEGOLD(0, 1000000)
	; State unchanged: the gate ran *before* the gold mutation.
	Assert(MutationGold = 0)
End Test

Test testChangeGoldGatePassesForPrivileged()
	InstallScript(1, 0, 0)
	ResetMutationCounters()
	MockBVM_CHANGEGOLD(0, 1000000)
	Assert(MutationGold = 1000000)
End Test

Test testChangeMoneyGateBlocksNonPrivileged()
	InstallScript(0, 0, 0)
	ResetMutationCounters()
	MockBVM_CHANGEMONEY(0, -500000)
	Assert(MutationMoney = 0)
End Test

Test testChangeMoneyGatePassesForPrivileged()
	InstallScript(1, 0, 0)
	ResetMutationCounters()
	MockBVM_CHANGEMONEY(0, -500000)
	Assert(MutationMoney = -500000)
End Test

Test testGiveXPGateBlocksNonPrivileged()
	InstallScript(0, 0, 0)
	ResetMutationCounters()
	MockBVM_GIVEXP(0, 999999999)
	Assert(MutationXP = 0)
End Test

Test testGiveXPGatePassesForPrivileged()
	InstallScript(1, 0, 0)
	ResetMutationCounters()
	MockBVM_GIVEXP(0, 100)
	Assert(MutationXP = 100)
End Test

Test testGiveKillXPGateBlocksNonPrivileged()
	InstallScript(0, 0, 0)
	ResetMutationCounters()
	MockBVM_GIVEKILLXP(0, 0)
	Assert(MutationKillXP = 0)
End Test

Test testGiveKillXPGatePassesForPrivileged()
	InstallScript(1, 0, 0)
	ResetMutationCounters()
	MockBVM_GIVEKILLXP(0, 0)
	Assert(MutationKillXP = 1)
End Test

Test testSetAttributeGateBlocksArbitraryTarget()
	InstallScript(0, 100, 0)
	ResetMutationCounters()
	MockBVM_SETATTRIBUTE(999, "Health", 0)
	Assert(MutationSetAttr = 0)
End Test

; THE EXPLOIT this gate exists to block. For Examine/Trade/RightClick/
; ItemScript spawns, ThreadScript("...", "Examine", Handle(clicker),
; Handle(NPC)) makes `SI\AI = Handle(clicker)`. A self-or-priv gate on
; Param1=clicker_handle would match SI\AI and let SetAttribute(clicker,
; "Health", 0) reach KillActor(...) on the clicker. The full-priv gate
; refuses regardless of how the target handle relates to SI\AI.
Test testSetAttributeGateBlocksKillingOwnAITarget()
	; Non-priv clicker-driven script: SI\AI = clicker handle (777 here).
	; The exploit call passes Param1 = 777 == SI\AI, which would defeat
	; a self-or-priv gate. The full-priv gate must still refuse.
	InstallScript(0, 777, 200)
	ResetMutationCounters()
	MockBVM_SETATTRIBUTE(777, "Health", 0)
	Assert(MutationSetAttr = 0)
End Test

Test testSetAttributeGateBlocksKillingOwnContextTarget()
	; Same exploit, but a script spawn shape where the lethal target
	; happens to be SI\AIContext. The gate must still refuse.
	InstallScript(0, 100, 500)
	ResetMutationCounters()
	MockBVM_SETATTRIBUTE(500, "Health", 0)
	Assert(MutationSetAttr = 0)
End Test

Test testSetAttributeGatePassesForPrivileged()
	InstallScript(1, 0, 0)
	ResetMutationCounters()
	MockBVM_SETATTRIBUTE(999, "Health", 42)
	Assert(MutationSetAttr = 42)
End Test

Test testChangeAttributeGateBlocksArbitraryTarget()
	; Same exploit shape via ChangeAttribute(target, "Health", -big%).
	InstallScript(0, 100, 0)
	ResetMutationCounters()
	MockBVM_CHANGEATTRIBUTE(999, "Health", -1000000)
	Assert(MutationChangeAttr = 0)
End Test

Test testChangeAttributeGateBlocksKillingOwnAITarget()
	; Mirror of the SetAttribute clicker-bypass test on the Change path.
	InstallScript(0, 777, 200)
	ResetMutationCounters()
	MockBVM_CHANGEATTRIBUTE(777, "Health", -1000000)
	Assert(MutationChangeAttr = 0)
End Test

Test testChangeAttributeGatePassesForPrivileged()
	InstallScript(1, 0, 0)
	ResetMutationCounters()
	MockBVM_CHANGEATTRIBUTE(999, "Health", -7)
	Assert(MutationChangeAttr = -7)
End Test

Test testRemoveZoneInstanceGateBlocksNonPrivileged()
	InstallScript(0, 0, 0)
	ResetMutationCounters()
	MockBVM_REMOVEZONEINSTANCE("MainZone", 5)
	Assert(MutationRemoveZone = 0)
End Test

Test testRemoveZoneInstanceGatePassesForPrivileged()
	InstallScript(1, 0, 0)
	ResetMutationCounters()
	MockBVM_REMOVEZONEINSTANCE("MainZone", 5)
	Assert(MutationRemoveZone = 1)
End Test
