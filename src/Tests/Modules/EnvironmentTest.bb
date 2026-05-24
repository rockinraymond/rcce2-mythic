Strict
EnableGC

; Logging stubs so Environment's SafeWrite/WriteLog calls resolve without
; pulling Modules\Logging.bb (which has its own UI/file deps).
Global MainLog = 0

Function WriteLog(LogID%, Message$)
End Function

Function SafeWriteOpen$(FinalPath$)
	Return FinalPath$ + ".tmp"
End Function

Function SafeWriteCommit%(TempPath$, FinalPath$, F)
	Return True
End Function

Function SafeWriteAbort(TempPath$, F)
End Function

Include "Modules\Environment.bb"

; TimeDelta is pure arithmetic over hour/minute pairs. The function has three
; branches (same-hour, forward-in-day, wraps-past-midnight); pin each one so
; later refactors of the day-cycle math can't drift the wall-clock delta the
; rest of the engine relies on (script timers, sun position, etc.).

Test testTimeDeltaWithinSameHourReturnsMinuteDifference()
	Assert(TimeDelta(10, 5, 10, 30) = 25)
	Assert(TimeDelta(0, 0, 0, 59) = 59)
	Assert(TimeDelta(23, 10, 23, 10) = 0)
End Test

Test testTimeDeltaForwardInSameDayCrossesHourBoundary()
	; 10:50 -> 11:05 = 15
	Assert(TimeDelta(10, 50, 11, 5) = 15)
	; 10:00 -> 12:00 = 120
	Assert(TimeDelta(10, 0, 12, 0) = 120)
	; 10:30 -> 11:00 = 30
	Assert(TimeDelta(10, 30, 11, 0) = 30)
End Test

Test testTimeDeltaSpansMidnightWhenEndHourBeforeStartHour()
	; 23:50 -> 00:10 = 20
	Assert(TimeDelta(23, 50, 0, 10) = 20)
	; 22:00 -> 01:00 = 180
	Assert(TimeDelta(22, 0, 1, 0) = 180)
End Test

