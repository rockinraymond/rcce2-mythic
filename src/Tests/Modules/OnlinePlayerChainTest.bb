Strict
; EnableGC intentionally omitted -- this test creates many small
; Type instances and chains them via Field references. With GC on,
; the runtime stack-overflows during chain walks (see
; feedback_blitzforge_test_handle_object_gc memory). Strict-only is
; sufficient for the chain-correctness assertions; no Handle/Object
; round-trip is needed.

; Regression test pinning the FirstOnlinePlayer / NextOnlinePlayer
; chain in Actors.bb (Phase 3 of the ActorByRNID multi-iteration
; initiative -- see PR #282 for Phase 1).
;
; The chain replaces a `For Each ActorInstance / If A2\RNID > 0` walk
; that ran on:
;   * The per-tick standard-update broadcast in GameServer.bb's
;     UpdateActorInstances (the dominant per-frame cost on a server
;     with many NPCs / spawned mobs).
;   * Six chat-broadcast loops in ServerNet.bb (/yell /gm /g /pm
;     /allplayers /warpother).
;
; Actors.bb's chain helpers (OnlinePlayerInsert / OnlinePlayerRemove)
; can't be exercised directly because ActorInstance pulls the whole
; network/world graph. Following the established replicated-gate
; pattern, this file rebuilds the chain logic against a tiny mock
; Type whose field shape matches ActorInstance's NextOnlinePlayer
; layout. Any production change to the helpers MUST update this file;
; the duplication is the trigger to refresh the test rationale.

; Mock actor used in place of ActorInstance. Only field we need is
; the chain-next link.
Type MockActor
	Field Name$
	Field NextOnline.MockActor
End Type

Global MockHead.MockActor = Null

; Replicates Actors.bb's OnlinePlayerInsert -- head-insert with
; presence dedup.
Function MockInsert(A.MockActor)
	If A = Null Then Return
	Local Cursor.MockActor = MockHead
	While Cursor <> Null
		If Cursor = A Then Return
		Cursor = Cursor\NextOnline
	Wend
	A\NextOnline = MockHead
	MockHead = A
End Function

; Replicates Actors.bb's OnlinePlayerRemove -- walk-to-find-predecessor.
Function MockRemove(A.MockActor)
	If A = Null Then Return
	If MockHead = Null Then Return
	If MockHead = A
		MockHead = A\NextOnline
		A\NextOnline = Null
		Return
	EndIf
	Local Prev.MockActor = MockHead
	While Prev\NextOnline <> Null
		If Prev\NextOnline = A
			Prev\NextOnline = A\NextOnline
			A\NextOnline = Null
			Return
		EndIf
		Prev = Prev\NextOnline
	Wend
End Function

; Helper: count chain length.
Function ChainLen%()
	Local Count% = 0
	Local Cursor.MockActor = MockHead
	While Cursor <> Null
		Count = Count + 1
		Cursor = Cursor\NextOnline
	Wend
	Return Count
End Function

; Helper: True if A is in the chain.
Function ChainContains%(A.MockActor)
	Local Cursor.MockActor = MockHead
	While Cursor <> Null
		If Cursor = A Then Return True
		Cursor = Cursor\NextOnline
	Wend
	Return False
End Function

Function ResetChain()
	; Walk the chain detaching nodes so they can be GC'd cleanly.
	Local Cursor.MockActor = MockHead
	Local CNext.MockActor = Null
	While Cursor <> Null
		CNext = Cursor\NextOnline
		Cursor\NextOnline = Null
		Cursor = CNext
	Wend
	MockHead = Null
End Function

; ====================================================================
; Insert: head-insert + dedup
; ====================================================================

Test testInsertOnEmptyChain()
	ResetChain()
	Local A.MockActor = New MockActor() : A\Name = "A"
	MockInsert(A)
	Assert(ChainLen%() = 1)
	Assert(MockHead = A)
End Test

Test testInsertManyHeadInserts()
	ResetChain()
	Local A.MockActor = New MockActor() : A\Name = "A"
	Local B.MockActor = New MockActor() : B\Name = "B"
	Local C.MockActor = New MockActor() : C\Name = "C"
	MockInsert(A)
	MockInsert(B)
	MockInsert(C)
	; Head-insert puts newest at front: C -> B -> A.
	Assert(MockHead = C)
	Assert(C\NextOnline = B)
	Assert(B\NextOnline = A)
	Assert(A\NextOnline = Null)
	Assert(ChainLen%() = 3)
End Test

Test testInsertDedupSilent()
	; Production OnlinePlayerInsert is idempotent -- a buggy caller
	; that inserts the same actor twice should not create a cycle.
	ResetChain()
	Local A.MockActor = New MockActor() : A\Name = "A"
	MockInsert(A)
	MockInsert(A)
	MockInsert(A)
	Assert(ChainLen%() = 1)
End Test

Test testInsertNullIsNoOp()
	ResetChain()
	MockInsert(Null)
	Assert(ChainLen%() = 0)
	Assert(MockHead = Null)
End Test

; ====================================================================
; Remove: head removal, middle removal, tail removal
; ====================================================================

Test testRemoveHead()
	ResetChain()
	Local A.MockActor = New MockActor() : A\Name = "A"
	Local B.MockActor = New MockActor() : B\Name = "B"
	Local C.MockActor = New MockActor() : C\Name = "C"
	MockInsert(A) : MockInsert(B) : MockInsert(C)
	; Chain: C -> B -> A.
	MockRemove(C)
	Assert(MockHead = B)
	Assert(ChainLen%() = 2)
	Assert(C\NextOnline = Null)
End Test

Test testRemoveMiddle()
	ResetChain()
	Local A.MockActor = New MockActor() : A\Name = "A"
	Local B.MockActor = New MockActor() : B\Name = "B"
	Local C.MockActor = New MockActor() : C\Name = "C"
	MockInsert(A) : MockInsert(B) : MockInsert(C)
	; Chain: C -> B -> A. Remove B.
	MockRemove(B)
	Assert(MockHead = C)
	Assert(C\NextOnline = A)
	Assert(A\NextOnline = Null)
	Assert(ChainLen%() = 2)
	Assert(B\NextOnline = Null)
End Test

Test testRemoveTail()
	ResetChain()
	Local A.MockActor = New MockActor() : A\Name = "A"
	Local B.MockActor = New MockActor() : B\Name = "B"
	Local C.MockActor = New MockActor() : C\Name = "C"
	MockInsert(A) : MockInsert(B) : MockInsert(C)
	; Chain: C -> B -> A. Remove A (tail).
	MockRemove(A)
	Assert(MockHead = C)
	Assert(C\NextOnline = B)
	Assert(B\NextOnline = Null)
	Assert(ChainLen%() = 2)
End Test

Test testRemoveSingleElement()
	ResetChain()
	Local A.MockActor = New MockActor() : A\Name = "A"
	MockInsert(A)
	MockRemove(A)
	Assert(MockHead = Null)
	Assert(ChainLen%() = 0)
End Test

Test testRemoveAbsentIsNoOp()
	; Production FreeActorInstance calls OnlinePlayerRemove on every
	; actor (NPCs, never-logged-in characters). The remove must be
	; safe when A isn't in the chain.
	ResetChain()
	Local A.MockActor = New MockActor() : A\Name = "A"
	Local B.MockActor = New MockActor() : B\Name = "B"
	MockInsert(A)
	; B was never inserted.
	MockRemove(B)
	Assert(MockHead = A)
	Assert(ChainLen%() = 1)
End Test

Test testRemoveFromEmptyChainIsNoOp()
	ResetChain()
	Local A.MockActor = New MockActor() : A\Name = "A"
	MockRemove(A)
	Assert(MockHead = Null)
	Assert(ChainLen%() = 0)
End Test

Test testRemoveNullIsNoOp()
	ResetChain()
	Local A.MockActor = New MockActor() : A\Name = "A"
	MockInsert(A)
	MockRemove(Null)
	Assert(ChainLen%() = 1)
End Test

; ====================================================================
; Lifecycle: insert + remove + re-insert (relogin pattern)
; ====================================================================

Test testReloginPattern()
	ResetChain()
	Local A.MockActor = New MockActor() : A\Name = "A"
	MockInsert(A)
	MockRemove(A)
	Assert(ChainLen%() = 0)
	MockInsert(A)
	Assert(ChainLen%() = 1)
	Assert(MockHead = A)
End Test

Test testManyLoginsLogoutsInterleaved()
	ResetChain()
	Local A.MockActor = New MockActor() : A\Name = "A"
	Local B.MockActor = New MockActor() : B\Name = "B"
	Local C.MockActor = New MockActor() : C\Name = "C"
	MockInsert(A)
	MockInsert(B)
	MockRemove(A)
	MockInsert(C)
	; Chain should be: C -> B (A removed).
	Assert(ChainLen%() = 2)
	Assert(ChainContains%(A) = False)
	Assert(ChainContains%(B) = True)
	Assert(ChainContains%(C) = True)
End Test

Test testRemoveAllOneByOne()
	ResetChain()
	Local A.MockActor = New MockActor() : A\Name = "A"
	Local B.MockActor = New MockActor() : B\Name = "B"
	Local C.MockActor = New MockActor() : C\Name = "C"
	MockInsert(A) : MockInsert(B) : MockInsert(C)
	MockRemove(B)
	MockRemove(C)
	MockRemove(A)
	Assert(MockHead = Null)
	Assert(ChainLen%() = 0)
End Test
