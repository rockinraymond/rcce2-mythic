; Tests for the bounded actor-sound channel pool (EmitActorSound) from
; Modules/Actors3D.bb -- the fix for issue #489 (per-step actor EmitSound
; calls were fire-and-forget and exhausted the audio backend's channels,
; hard-crashing the client after sustained play).
;
; Actors3D.bb can't be Included into a test build (it pulls in the whole 3D
; media/animation graph). As with ClampFloatTest.bb, this test REPLICATES the
; pool's decision logic verbatim and pins the *behaviour*: the cursor-wrap,
; the LRU StopChannel-on-reuse that bounds the live channel count, and the
; Snd=0 short-circuit. A refactor that changes EmitActorSound has to update
; both the production copy and this duplicate -- that is the trigger to
; refresh this test.
;
; The three audio primitives (EmitSound / ChannelPlaying / StopChannel) are
; engine commands that don't exist in an offline test link, so they are
; replaced with Stub* shims that record what production would have asked the
; backend to do. The pool bookkeeping around them is byte-for-byte the
; production shape. The pool size is reduced to 4 here (production uses 24);
; the wrap/reclaim invariant is identical at any size and 4 keeps the
; assertions legible.
;
; NOT Strict: matches the non-Strict production module and lets the
; replicated Dim pool array be written to inside the function (Strict mode
; rejects Dim-array element writes inside Functions).

Const TPoolSize = 4
Dim TChannel(TPoolSize - 1)
Global TCursor = 0

; --- stubbed audio backend ------------------------------------------------
Global StubNextCh   = 100   ; mints a fresh, nonzero "channel handle" per emit
Global StubStopCount = 0    ; how many StopChannel calls production made
Global StubLastStopped = 0  ; the handle of the most recent StopChannel
Global StubAllPlaying = True ; controls what ChannelPlaying reports

Function StubEmitSound%()
	StubNextCh = StubNextCh + 1
	Return StubNextCh
End Function

Function StubChannelPlaying%(ch)
	If ch = 0 Then Return False
	Return StubAllPlaying
End Function

Function StubStopChannel(ch)
	StubStopCount = StubStopCount + 1
	StubLastStopped = ch
End Function

; --- replicated production logic (EmitActorSound, Actors3D.bb) -------------
; EN is dropped here (the stub emit ignores the entity); everything else is
; the production shape.
Function EmitActorSoundT%(Snd)
	If Snd = 0 Then Return 0
	If TChannel(TCursor) <> 0
		If StubChannelPlaying(TChannel(TCursor))
			StubStopChannel(TChannel(TCursor))
		EndIf
	EndIf
	Local Ch = StubEmitSound()
	TChannel(TCursor) = Ch
	TCursor = (TCursor + 1) Mod TPoolSize
	Return Ch
End Function

Function ResetPool()
	Local i
	For i = 0 To TPoolSize - 1
		TChannel(i) = 0
	Next
	TCursor = 0
	StubNextCh = 100
	StubStopCount = 0
	StubLastStopped = 0
	StubAllPlaying = True
End Function


; A missing/unregistered sound (GetSound returns 0) is skipped entirely: no
; channel is minted, the cursor doesn't advance, nothing is stopped. This is
; also a small improvement over the old raw EmitSound(0, ...) call.
Test testSndZeroShortCircuits()
	ResetPool()
	Assert(EmitActorSoundT(0) = 0)
	Assert(TCursor = 0)
	Assert(StubStopCount = 0)
End Test

; Filling the pool for the first time consumes each slot once and reclaims
; nothing -- every slot was empty (0). After exactly TPoolSize emits the
; cursor has wrapped back to 0.
Test testFirstFillReclaimsNothing()
	ResetPool()
	Local i, ch
	For i = 1 To TPoolSize
		ch = EmitActorSoundT(7)
		Assert(ch <> 0)
	Next
	Assert(StubStopCount = 0)
	Assert(TCursor = 0)
End Test

; The cursor never leaves [0, TPoolSize) no matter how many sounds emit --
; this is what keeps the Dim index in bounds for the life of the session.
Test testCursorStaysInBounds()
	ResetPool()
	Local i
	For i = 1 To TPoolSize * 3 + 1
		EmitActorSoundT(7)
		Assert(TCursor >= 0)
		Assert(TCursor < TPoolSize)
	Next
End Test

; Once the pool is full, the next emit reclaims the OLDEST slot's channel
; (LRU), and the one after reclaims the next-oldest. Channels are minted
; 101,102,103,104 for the first fill, so emit #5 stops 101 and #6 stops 102.
Test testReclaimsOldestOnWrap()
	ResetPool()
	Local i
	For i = 1 To TPoolSize
		EmitActorSoundT(7)
	Next
	Assert(StubStopCount = 0)

	EmitActorSoundT(7)
	Assert(StubStopCount = 1)
	Assert(StubLastStopped = 101)

	EmitActorSoundT(7)
	Assert(StubStopCount = 2)
	Assert(StubLastStopped = 102)
End Test

; The core invariant from issue #489: live channels are capped at the pool
; size regardless of how many sounds play. After K emits (K > TPoolSize)
; production has stopped exactly K - TPoolSize channels, so at most TPoolSize
; are ever live at once.
Test testLiveChannelCountIsCapped()
	ResetPool()
	Local i
	Local K = TPoolSize * 5 + 3
	For i = 1 To K
		EmitActorSoundT(7)
	Next
	Assert(StubStopCount = K - TPoolSize)
End Test

; If a slot's previous sound already finished on its own (ChannelPlaying
; False), no StopChannel is issued -- the slot is simply overwritten. The
; reclaim is conditional, not unconditional, so finished sounds aren't
; needlessly re-stopped, and the cursor still stays bounded.
Test testNoStopWhenSlotAlreadyFinished()
	ResetPool()
	StubAllPlaying = False
	Local i
	For i = 1 To TPoolSize * 2
		EmitActorSoundT(7)
		Assert(TCursor >= 0)
		Assert(TCursor < TPoolSize)
	Next
	Assert(StubStopCount = 0)
End Test
