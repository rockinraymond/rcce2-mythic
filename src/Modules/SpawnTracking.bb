; Rebuilds AreaInstance\Spawned[] from the live ActorInstance list.
;
; This is primarily used when the server lock/unlock cycle leaves stale spawn
; occupancy behind after actors were cleaned up through a non-death path. By
; reconstructing the counts from current actors, normal spawn scheduling can
; repopulate any missing NPCs instead of treating those slots as permanently
; full.
Function SyncAreaSpawnCounts()

	For AInstance.AreaInstance = Each AreaInstance
		For i = 0 To 999
			AInstance\Spawned[i] = 0
		Next
	Next

	For AI.ActorInstance = Each ActorInstance
		If AI\SourceSP > -1
			AInstance.AreaInstance = Object.AreaInstance(AI\ServerArea)
			If AInstance <> Null
				AInstance\Spawned[AI\SourceSP] = AInstance\Spawned[AI\SourceSP] + 1
			EndIf
		EndIf
	Next

End Function
