; Everything the server needs to know about an area (except water)
Type Area
	; Area name
	Field Name$
	; Environment
	Field WeatherChance[4]
	Field Outdoors
	Field WeatherLink$, WeatherLinkArea.Area
	; Area scripts
	Field EntryScript$, ExitScript$
	; Script triggers
	Field TriggerX#[149], TriggerY#[149], TriggerZ#[149], TriggerSize#[149], TriggerScript$[149], TriggerMethod$[149]
	; Waypoints
	Field WaypointX#[1999], WaypointY#[1999], WaypointZ#[1999]
	Field PrevWaypoint[1999], NextWaypointA[1999], NextWaypointB[1999]
	Field WaypointPause[1999]
	; Portals
	Field PortalName$[99], PortalLinkArea$[99], PortalLinkName$[99]
	Field PortalX#[99], PortalY#[99], PortalZ#[99], PortalSize#[99], PortalYaw#[99]
	; Spawn points
	Field SpawnActor[999], SpawnWaypoint[999], SpawnSize#[999], SpawnScript$[999], SpawnActorScript$[999], SpawnDeathScript$[999]
	Field SpawnFrequency[999], SpawnMax[999], SpawnRange#[999]
	; Is PvP allowed
	Field PvP
	; Gravity strength (0-1000)
	Field Gravity
	; Server-side collision map for NPC pathing and AI steering
	Field CollisionOriginX#, CollisionOriginZ#
	Field CollisionGridX[40000], CollisionGridZ[40000]
	Field CollisionRoot.ServerCollisionBox
	; Track instances
	Field Instances.AreaInstance[99]
End Type

Type ServerCollisionBox
	Field Area.Area
	Field NextBox.ServerCollisionBox
	Field X#, Y#, Z#
	Field SizeX#, SizeY#, SizeZ#
End Type

;
; ServerMoveTarget globals removed - move targets are now written
; into an actor instance's `DestX#`/`DestZ#` by ServerFindMoveTarget.

; Water areas for damaging things
Type ServerWater
	Field Area.Area
	Field X#, Y#, Z#
	Field Width#, Depth#
	Field Damage, DamageType
End Type

; Each area instance may have up to 500 player owned items of scenery (e.g. chests, doors, etc.) {##}
;Type OwnedScenery
;	Field InventorySize
;	Field Inventory.Inventory
;	Field AccountName$, CharNumber
;End Type

; Instancing structure
Type AreaInstance
	Field Area.Area
	Field ID
	Field FirstInZone.ActorInstance ; Head of linked list containing all actor instances in a zone
	Field CurrentWeather, CurrentWeatherTime
	Field SpawnLast[999], Spawned[999]
	Field LastSpawnedItem.ItemInstance ;used for creating magic items
	;Field OwnedScenery.OwnedScenery[499] ; {##}
End Type

; Updates weather for an area
Function UpdateWeather(A.AreaInstance)

	A\CurrentWeatherTime = A\CurrentWeatherTime - 1

	; Time to update the weather for this area
	If A\CurrentWeatherTime <= 0
		; Get weather from linked area
		If A\Area\WeatherLinkArea <> Null
			A\CurrentWeatherTime = A\Area\WeatherLinkArea\Instances[0]\CurrentWeatherTime
			A\CurrentWeather = A\Area\WeatherLinkArea\Instances[0]\CurrentWeather
		; Choose own weather from probabilities
		Else
			A\CurrentWeatherTime = Rand(2500, 10000)
			A\CurrentWeather = 0
			NewWeather = Rand(1, 100)
			Min = 0
			For i = 0 To 4
				If A\Area\WeatherChance[i] > 0
					Max = Min + A\Area\WeatherChance[i]
					If NewWeather >= Min And NewWeather < Max Then A\CurrentWeather = i + 1 : Exit
					Min = Max
				EndIf
			Next
		EndIf

		; Inform players in this area
		AI.ActorInstance = A\FirstInZone
		While AI <> Null
			If AI\RNID > 0 Then RCE_Send(Host, AI\RNID, P_WeatherChange, RCE_StrFromInt$(Handle(A), 4) + RCE_StrFromInt$(A\CurrentWeather, 1), True)
			AI = AI\NextInZone
		Wend
	EndIf

End Function

; Returns true when there is an unobstructed path between two 2D points for an actor
Function ServerHasLineOfSight(A.Area, X1#, Z1#, X2#, Z2#, Radius#)
	If A = Null Then Return False
	DX# = X2# - X1#
	DZ# = Z2# - Z1#
	Dist# = Sqr(DX# * DX# + DZ# * DZ#)
	If Dist# = 0.0 Then Return True
	StepSize# = 0.5
	Steps = Ceil(Dist# / StepSize#)
	For s = 0 To Steps
		t# = Float#(s) / Float#(Steps)
		TX# = X1# + (DX# * t#)
		TZ# = Z1# + (DZ# * t#)
		If ServerIsBlocked(A, TX#, TZ#, Radius#) = True Then Return False
	Next
	Return True
End Function


; Creates a new blank area
Function ServerCreateArea.Area()

	A.Area = New Area
	For i = 0 To 1999
		A\PrevWaypoint[i] = 2005
		A\NextWaypointA[i] = 2005
		A\NextWaypointB[i] = 2005
		If i < 1000 Then A\SpawnFrequency[i] = 10
	Next
	A\Gravity = 300
	;ClearServerCollisionMap(A)
	ServerCreateAreaInstance(A, 0)
	Return A

End Function

; Creates a new instance of an area
Function ServerCreateAreaInstance.AreaInstance(Ar.Area, ID)

	; New instance
	AInstance.AreaInstance = New AreaInstance
	Ar\Instances[ID] = AInstance
	AInstance\Area = Ar
	AInstance\ID = ID

	; Initial spawn point times
	For i = 0 To 999
		AInstance\SpawnLast[i] = MilliSecs()
	Next

	; Copy ownable scenery data from default instance [@@@]
	;If ID > 0
	;	For i = 0 To 499
	;		If Ar\Instances[0]\OwnedScenery[i] <> Null
	;			AInstance\OwnedScenery[i] = New OwnedScenery
	;			AInstance\OwnedScenery[i]\InventorySize = Ar\Instances[0]\OwnedScenery[i]\InventorySize
	;			If AInstance\OwnedScenery[i]\InventorySize > 0 Then AInstance\OwnedScenery[i]\Inventory = New Inventory
	;		EndIf
	;	Next
	;EndIf

	; Done
	Return AInstance

End Function

; Finds an area by the name
Function FindArea.Area(Name$)

	Name$ = Upper$(Name$)
	For A.Area = Each Area
		If Upper$(A\Name$) = Name$ Then Return A
	Next

End Function

; Unloads all server data for an area
Function ServerUnloadArea(A.Area)

	For W.ServerWater = Each ServerWater
		If W\Area = A Then Delete(W)
	Next
	;For j = 0 To 99 {##}
	;	If A\Instances[j] <> Null
	;		For i = 0 To 499
	;			If A\Instances[j]\OwnedScenery[i] <> Null
	;				If A\Instances[j]\OwnedScenery[i]\Inventory <> Null Then Delete A\Instances[j]\OwnedScenery[i]\Inventory
	;				Delete A\Instances[j]\OwnedScenery[i]
	;			EndIf
	;		Next
	;		Delete A\Instances[j]
	;	EndIf
	;Next
	Delete(A)

End Function

; Converts a 2D grid coordinate into a flat Blitz-compatible index.
Function ServerCollisionCellIndex(cx, cz)
	If cx < 0 Or cx >= 200 Or cz < 0 Or cz >= 200 Then Return -1
	Return (cx * 200) + cz
End Function

; Clears all server-side collision blocks for an area.
Function ClearServerCollisionMap(A.Area)
	If A = Null Then Return
	For i = 0 To 39999
		A\CollisionGridX[i] = 0
		A\CollisionGridZ[i] = 0
	Next
	A\CollisionOriginX# = 0.0
	A\CollisionOriginZ# = 0.0
	If A\CollisionRoot <> Null
		B.ServerCollisionBox = A\CollisionRoot
		While B <> Null
			NextB.ServerCollisionBox = B\NextBox
			Delete(B)
			B = NextB
		Wend
		A\CollisionRoot = Null
	EndIf
End Function

; Adds a collision box to the server occupancy map for AI pathing. This matches the client area collision boxes in spirit: a box volume that blocks movement.
Function ServerAddCollisionBox(A.Area, X#, Y#, Z#, SizeX#, SizeY#, SizeZ#)
	If A = Null Then Return
	CellSize# = 0.15
	If A\CollisionRoot = Null
		A\CollisionOriginX# = X# - (SizeX# * 0.5)
		A\CollisionOriginZ# = Z# - (SizeZ# * 0.5)
	Else
		MinX# = X# - (SizeX# * 0.5)
		MinZ# = Z# - (SizeZ# * 0.5)
		If MinX# < A\CollisionOriginX# Then A\CollisionOriginX# = MinX#
		If MinZ# < A\CollisionOriginZ# Then A\CollisionOriginZ# = MinZ#
	EndIf

	B.ServerCollisionBox = New ServerCollisionBox
	B\Area = A
	B\X# = X#
	B\Y# = Y#
	B\Z# = Z#
	B\SizeX# = SizeX#
	B\SizeY# = SizeY#
	B\SizeZ# = SizeZ#
	B\NextBox = A\CollisionRoot
	A\CollisionRoot = B

	MinCellX = Int((X# - (SizeX# * 0.5) - A\CollisionOriginX#) / 0.15)
	MaxCellX = Int((X# + (SizeX# * 0.5) - A\CollisionOriginX#) / 0.15)
	MinCellZ = Int((Z# - (SizeZ# * 0.5) - A\CollisionOriginZ#) / 0.15)
	MaxCellZ = Int((Z# + (SizeZ# * 0.5) - A\CollisionOriginZ#) / 0.15)

	For cx = MinCellX To MaxCellX
		If cx >= 0 And cx < 200
			For cz = MinCellZ To MaxCellZ
				If cz >= 0 And cz < 200
					Index = ServerCollisionCellIndex(cx, cz)
					If Index >= 0
						A\CollisionGridX[Index] = 1
						A\CollisionGridZ[Index] = 1
					EndIf
				EndIf
			Next
		EndIf
	Next
End Function

; Returns true when the given world-space position is inside a server collision block.
Function ServerIsBlocked(A.Area, X#, Z#, Radius# = 1.0)
	If A = Null Then Return False
	CellSize# = 0.15
	CellRadius = Ceil(Radius# / CellSize#)
	CellX = Int((X# - A\CollisionOriginX#) / CellSize#)
	CellZ = Int((Z# - A\CollisionOriginZ#) / CellSize#)
	For cx = CellX - CellRadius To CellX + CellRadius
		If cx >= 0 And cx < 200
			For cz = CellZ - CellRadius To CellZ + CellRadius
				If cz >= 0 And cz < 200
					Index = ServerCollisionCellIndex(cx, cz)
					If Index >= 0
						If A\CollisionGridX[Index] = 1 Or A\CollisionGridZ[Index] = 1 Then Return True
					EndIf
				EndIf
			Next
		EndIf
	Next
	Return False
End Function

; Finds a nearby valid destination around a blocked goal using the server collision map.
Function ServerFindMoveTarget(A.Area, AI.ActorInstance, StartX#, StartZ#, GoalX#, GoalZ#, Radius#)
	; Write the chosen move target directly into the supplied actor's DestX#/DestZ#.
	If AI <> Null
		AI\DestX# = GoalX#
		AI\DestZ# = GoalZ#
	EndIf
	If A = Null Then Return
	If ServerIsBlocked(A, GoalX#, GoalZ#, Radius#) = False Then Return
	SearchRange# = 8.0
	For SearchStep# = 1.0 To SearchRange# Step 0.5
		For Dir = 0 To 15
			Angle# = Float#(Dir) * 22.5
			TestX# = GoalX# + (Cos#(Angle#) * SearchStep#)
			TestZ# = GoalZ# + (Sin#(Angle#) * SearchStep#)
			If ServerIsBlocked(A, TestX#, TestZ#, Radius#) = False
				If AI <> Null
					AI\DestX# = TestX#
					AI\DestZ# = TestZ#
				EndIf
				Return
			EndIf
		Next
	Next
	If AI <> Null
		AI\DestX# = StartX#
		AI\DestZ# = StartZ#
	EndIf
End Function

; Returns the path to the separate server-only collision file for an area.
Function ServerCollisionDataPath$(AreaName$)
	Return "Data\Server Data\Areas\" + AreaName$ + "_collision.dat"
End Function

; Returns true when the server-only collision file exists for this area.
Function ServerCollisionDataExists(AreaName$)
	F = ReadFile(ServerCollisionDataPath$(AreaName$))
	If F = 0 Then Return False
	CloseFile(F)
	Return True
End Function

; Saves the server collision box data to a separate, server-only file so it never touches
; the client area file or the base server area save format.
Function ServerSaveAreaCollisionMap(A.Area)
	If A = Null Then Return
	Path$ = ServerCollisionDataPath$(A\Name$)
	F = WriteFile(Path$)
	If F = 0
		If MainLog <> 0 Then WriteLog(MainLog, "ServerSaveAreaCollisionMap: could not open '" + Path$ + "'.")
		Return
	EndIf

	ClearServerCollisionMap(A)

	; Include explicit collision boxes from the area.
	For C.ColBox = Each ColBox
		If C\EN <> 0
			X# = EntityX#(C\EN, True)
			Y# = EntityY#(C\EN, True)
			Z# = EntityZ#(C\EN, True)
			ServerAddCollisionBox(A, X#, Y#, Z#, C\ScaleX#, C\ScaleY#, C\ScaleZ#)
		EndIf
	Next

	; Include scenery that is currently marked as collidable.
	For S.Scenery = Each Scenery
		If S\EN <> 0
			Collides = GetEntityType(S\EN)
			If Collides <> 0
				X# = EntityX#(S\EN, True)
				Y# = EntityY#(S\EN, True)
				Z# = EntityZ#(S\EN, True)
				Width# = MeshWidth#(S\EN) * S\ScaleX#
				Height# = MeshHeight#(S\EN) * S\ScaleY#
				Depth# = MeshDepth#(S\EN) * S\ScaleZ#
				If Width# <= 0.0 Then Width# = 1.0
				If Height# <= 0.0 Then Height# = 1.0
				If Depth# <= 0.0 Then Depth# = 1.0
				ServerAddCollisionBox(A, X#, Y#, Z#, Width#, Height#, Depth#)
			EndIf
		EndIf
	Next

	Count = 0
	B.ServerCollisionBox = A\CollisionRoot
	While B <> Null
		Count = Count + 1
		B = B\NextBox
	Wend
	WriteInt(F, Count)

	B = A\CollisionRoot
	While B <> Null
		WriteFloat(F, B\X#)
		WriteFloat(F, B\Y#)
		WriteFloat(F, B\Z#)
		WriteFloat(F, B\SizeX#)
		WriteFloat(F, B\SizeY#)
		WriteFloat(F, B\SizeZ#)
		B = B\NextBox
	Wend
	CloseFile(F)

	If MainLog <> 0 Then WriteLog(MainLog, "ServerSaveAreaCollisionMap: saved " + Str$(Count) + " boxes for area '" + A\Name$ + "'.")
End Function

; Loads the server collision box data from the separate collision file.
Function ServerLoadAreaCollisionMap(A.Area, AreaName$)
	If A = Null Then Return
	Path$ = ServerCollisionDataPath$(AreaName$)
	F = ReadFile(Path$)
	If F = 0
		If MainLog <> 0 Then WriteLog(MainLog, "ServerLoadAreaCollisionMap: no collision file for '" + AreaName$ + "'.")
		Return
	EndIf

	ClearServerCollisionMap(A)
	Count = ReadInt(F)
	For i = 1 To Count
		X# = ReadFloat#(F)
		Y# = ReadFloat#(F)
		Z# = ReadFloat#(F)
		SizeX# = ReadFloat#(F)
		SizeY# = ReadFloat#(F)
		SizeZ# = ReadFloat#(F)
		ServerAddCollisionBox(A, X#, Y#, Z#, SizeX#, SizeY#, SizeZ#)
	Next
	CloseFile(F)

	If MainLog <> 0 Then WriteLog(MainLog, "ServerLoadAreaCollisionMap: loaded " + Str$(Count) + " boxes for area '" + AreaName$ + "'.")
End Function

; Ensures the server collision map is present for a zone. It comes from the separate,
; server-only collision file rather than the current client/server area save formats.
Function ServerEnsureAreaCollisionMap(A.Area, AreaName$)
	If A = Null
		If MainLog <> 0 Then WriteLog(MainLog, "ServerEnsureAreaCollisionMap: area was null for '" + AreaName$ + "'.")
		Return
	EndIf
	If ServerCollisionDataExists(AreaName$) = True
		If MainLog <> 0 Then WriteLog(MainLog, "ServerEnsureAreaCollisionMap: loading collision map from separate file for '" + AreaName$ + "'.")
		ServerLoadAreaCollisionMap(A, AreaName$)
	Else
		If MainLog <> 0 Then WriteLog(MainLog, "ServerEnsureAreaCollisionMap: no separate collision file found for '" + AreaName$ + "'; skipping server collision setup.")
	EndIf
End Function

; Loads the server data for an area
Function ServerLoadArea.Area(Name$)

	If MainLog <> 0 Then WriteLog(MainLog, "ServerLoadArea: loading area '" + Name$ + "'.")

	F = ReadFile("Data\Server Data\Areas\" + Name$ + ".dat")
	If F = 0
		If MainLog <> 0 Then WriteLog(MainLog, "ServerLoadArea: missing server data file for '" + Name$ + "'.")
		Return Null
	EndIf

		A.Area = New Area
		A\Name$ = Name$
		;ClearServerCollisionMap(A)
		For i = 0 To 4 : A\WeatherChance[i] = ReadByte(F) : Next
		A\EntryScript$ = ReadString$(F)
		A\ExitScript$  = ReadString$(F)
		A\PvP          = ReadByte(F)
		A\Gravity      = ReadShort(F)
		A\Outdoors     = ReadByte(F)
		A\WeatherLink$ = ReadString$(F)
		For i = 0 To 149
			A\TriggerX#[i]      = ReadFloat#(F)
			A\TriggerY#[i]      = ReadFloat#(F)
			A\TriggerZ#[i]      = ReadFloat#(F)
			A\TriggerSize#[i]   = ReadFloat#(F)
			A\TriggerScript$[i] = ReadString$(F)
			A\TriggerMethod$[i] = ReadString$(F)
		Next
		For i = 0 To 1999
			A\WaypointX#[i]    = ReadFloat#(F)
			A\WaypointY#[i]    = ReadFloat#(F)
			A\WaypointZ#[i]    = ReadFloat#(F)
			A\NextWaypointA[i] = ReadShort(F)
			A\NextWaypointB[i] = ReadShort(F)
			A\PrevWaypoint[i]  = ReadShort(F)
			A\WaypointPause[i] = ReadInt(F)
		Next
		For i = 0 To 99
			A\PortalName$[i]     = ReadString$(F)
			A\PortalLinkArea$[i] = ReadString$(F)
			A\PortalLinkName$[i] = ReadString$(F)
			A\PortalX#[i]        = ReadFloat#(F)
			A\PortalY#[i]        = ReadFloat#(F)
			A\PortalZ#[i]        = ReadFloat#(F)
			A\PortalSize#[i]     = ReadFloat#(F)
			A\PortalYaw#[i]      = ReadFloat#(F)
		Next
		For i = 0 To 999
			A\SpawnActor[i]        = ReadShort(F)
			A\SpawnWaypoint[i]     = ReadShort(F)
			A\SpawnSize#[i]        = ReadFloat#(F)
			A\SpawnScript$[i]      = ReadString$(F)
			A\SpawnActorScript$[i] = ReadString$(F)
			A\SpawnDeathScript$[i] = ReadString$(F)
			A\SpawnMax[i]          = ReadShort(F)
			A\SpawnFrequency[i]    = ReadShort(F)
			A\SpawnRange#[i]       = ReadFloat#(F)
		Next
		Waters = ReadShort(F)
		For i = 1 To Waters
			W.ServerWater = New ServerWater
			W\Area = A
			W\X# = ReadFloat#(F)
			W\Y# = ReadFloat#(F)
			W\Z# = ReadFloat#(F)
			W\Width#     = ReadFloat#(F)
			W\Depth#     = ReadFloat#(F)
			W\Damage     = ReadShort(F)
			W\DamageType = ReadShort(F)
		Next

	CloseFile(F)

	If MainLog <> 0 Then WriteLog(MainLog, "ServerLoadArea: loaded area '" + Name$ + "' and finished reading server data.")


	 ServerEnsureAreaCollisionMap(A, Name$)
	If MainLog <> 0 Then WriteLog(MainLog, "ServerLoadArea: created server collision for area '" + Name$ + "'.")

	; Create default instance (#0)
	AInstance.AreaInstance = ServerCreateAreaInstance(A, 0)

	; Load in any scenery ownerships {##}
	;For k = 0 To 99
	;	F = ReadFile("Data\Server Data\Areas\Ownerships\" + Name$ + " (" + Str$(k) + ") Ownerships.dat")
	;	If F <> 0
;
			; Create instance if required
	;		If k > 0 Then AInstance = ServerCreateAreaInstance(A, k)
;
			; Load data into instance
	;		For i = 0 To 499
	;			Exists = ReadByte(F)
	;			If Exists = 1
	;				AInstance\OwnedScenery[i] = New OwnedScenery
	;				AInstance\OwnedScenery[i]\AccountName$ = ReadString$(F)
	;				AInstance\OwnedScenery[i]\CharNumber = ReadByte(F)
	;				AInstance\OwnedScenery[i]\InventorySize = ReadByte(F)
	;				If AInstance\OwnedScenery[i]\InventorySize > 0
	;					AInstance\OwnedScenery[i]\Inventory = New Inventory
	;					For j = 0 To AInstance\OwnedScenery[i]\InventorySize - 1
	;						AInstance\OwnedScenery[i]\Inventory\Items[j] = ReadItemInstance(F)
	;						AInstance\OwnedScenery[i]\Inventory\Amounts[j] = ReadShort(F)
	;					Next
	;				EndIf
	;			EndIf
	;		Next
	;		CloseFile(F)
;
;		EndIf
;	Next

	Return A

End Function

; Saves the server data for an area
Function ServerSaveArea(A.Area)

	; Save map data
	F = WriteFile("Data\Server Data\Areas\" + A\Name$ + ".dat")
	If F = 0 Then Return False

		For i = 0 To 4 : WriteByte F, A\WeatherChance[i] : Next
		WriteString(F, A\EntryScript$)
		WriteString(F, A\ExitScript$)
		WriteByte(F,   A\PvP)
		WriteShort(F,  A\Gravity)
		WriteByte( F,  A\Outdoors)
		WriteString(F, A\WeatherLink$)
		For i = 0 To 149
			WriteFloat(F, A\TriggerX#[i])
			WriteFloat(F, A\TriggerY#[i])
			WriteFloat(F, A\TriggerZ#[i])
			WriteFloat(F, A\TriggerSize#[i])
			WriteString(F, A\TriggerScript$[i])
			WriteString(F, A\TriggerMethod$[i])
		Next
		For i = 0 To 1999
			WriteFloat(F, A\WaypointX#[i])
			WriteFloat(F, A\WaypointY#[i])
			WriteFloat(F, A\WaypointZ#[i])
			WriteShort(F, A\NextWaypointA[i])
			WriteShort(F, A\NextWaypointB[i])
			WriteShort(F, A\PrevWaypoint[i])
			WriteInt(F, A\WaypointPause[i])
		Next
		For i = 0 To 99
			WriteString(F, A\PortalName$[i])
			WriteString(F, A\PortalLinkArea$[i])
			WriteString(F, A\PortalLinkName$[i])
			WriteFloat(F, A\PortalX#[i])
			WriteFloat(F, A\PortalY#[i])
			WriteFloat(F, A\PortalZ#[i])
			WriteFloat(F, A\PortalSize#[i])
			WriteFloat(F, A\PortalYaw#[i])
		Next
		For i = 0 To 999
			WriteShort(F, A\SpawnActor[i])
			WriteShort(F, A\SpawnWaypoint[i])
			WriteFloat(F, A\SpawnSize#[i])
			WriteString(F, A\SpawnScript$[i])
			WriteString(F, A\SpawnActorScript$[i])
			WriteString(F, A\SpawnDeathScript$[i])
			WriteShort(F, A\SpawnMax[i])
			WriteShort(F, A\SpawnFrequency[i])
			WriteFloat(F, A\SpawnRange#[i])
		Next

		; Water areas
		Count = 0
		For W.ServerWater = Each ServerWater
			If W\Area = A Then Count = Count + 1
		Next
		WriteShort(F, Count)
		For W.ServerWater = Each ServerWater
			If W\Area = A
				WriteFloat(F, W\X#)
				WriteFloat(F, W\Y#)
				WriteFloat(F, W\Z#)
				WriteFloat(F, W\Width#)
				WriteFloat(F, W\Depth#)
				WriteShort(F, W\Damage)
				WriteShort(F, W\DamageType)
			EndIf
		Next

	CloseFile(F)

	;ServerSaveAreaOwnerships(A) {##}

	Return True

End Function

; Copies an area object exactly
Function ServerCopyArea.Area(A.Area)

	; Create area
	NewA.Area = New Area
	NewA\Name$ = "Copied zone"
	AInstance.AreaInstance = New AreaInstance
	NewA\Instances[0] = AInstance
	AInstance\Area = NewA

	; Copy data
	For i = 0 To 4
		NewA\WeatherChance[i] = A\WeatherChance[i]
	Next
	NewA\Outdoors = A\Outdoors
	NewA\WeatherLink$ = A\WeatherLink$
	;For i = 0 To 499 ;{##}
	;	If A\Instances[0]\OwnedScenery[i] <> Null
	;		NewA\Instances[0]\OwnedScenery[i] = New OwnedScenery
	;		NewA\Instances[0]\OwnedScenery[i]\InventorySize = A\Instances[0]\OwnedScenery[i]\InventorySize
	;		If NewA\Instances[0]\OwnedScenery[i]\InventorySize > 0 Then NewA\Instances[0]\OwnedScenery[i]\Inventory = New Inventory
	;	EndIf
	;Next
	NewA\EntryScript$ = A\EntryScript$
	NewA\ExitScript$ = A\ExitScript$
	For i = 0 To 149
		NewA\TriggerX#[i] = A\TriggerX#[i]
		NewA\TriggerY#[i] = A\TriggerY#[i]
		NewA\TriggerZ#[i] = A\TriggerZ#[i]
		NewA\TriggerSize#[i] = A\TriggerSize#[i]
		NewA\TriggerScript$[i] = A\TriggerScript$[i]
		NewA\TriggerMethod$[i] = A\TriggerMethod$[i]
	Next
	For i = 0 To 1999
		NewA\WaypointX#[i] = A\WaypointX#[i]
		NewA\WaypointY#[i] = A\WaypointY#[i]
		NewA\WaypointZ#[i] = A\WaypointZ#[i]
		NewA\PrevWaypoint[i] = A\PrevWaypoint[i]
		NewA\NextWaypointA[i] = A\NextWaypointA[i]
		NewA\NextWaypointB[i] = A\NextWaypointB[i]
		NewA\WaypointPause[i] = A\WaypointPause[i]
	Next
	For i = 0 To 999
		NewA\SpawnActor[i] = A\SpawnActor[i]
		NewA\SpawnWaypoint[i] = A\SpawnWaypoint[i]
		NewA\SpawnSize#[i] = A\SpawnSize#[i]
		NewA\SpawnScript$[i] = A\SpawnScript$[i]
		NewA\SpawnActorScript$[i] = A\SpawnActorScript$[i]
		NewA\SpawnDeathScript$[i] = A\SpawnDeathScript$[i]
		NewA\SpawnFrequency[i] = A\SpawnFrequency[i]
		NewA\SpawnMax[i] = A\SpawnMax[i]
	Next
	For i = 0 To 99
		NewA\PortalName$[i] = A\PortalName$[i]
		NewA\PortalLinkArea$[i] = A\PortalLinkArea$[i]
		NewA\PortalLinkName$[i] = A\PortalLinkName$[i]
		NewA\PortalX#[i] = A\PortalX#[i]
		NewA\PortalY#[i] = A\PortalY#[i]
		NewA\PortalZ#[i] = A\PortalZ#[i]
		NewA\PortalSize#[i] = A\PortalSize#[i]
		NewA\PortalYaw#[i] = A\PortalYaw#[i]
	Next
	NewA\PvP = A\PvP
	NewA\Gravity = A\Gravity

	Return NewA

End Function

; Save scenery ownerships {##}
;Function ServerSaveAreaOwnerships(Ar.Area)
;
;	For j = 0 To 99
;		; Find whether this instance has any ownerships which need saving
;		If j = 0
;			SaveInstance = True
;		Else
;			SaveInstance = False
;			If Ar\Instances[j] <> Null
;				For i = 0 To 499
;					If Ar\Instances[j]\OwnedScenery[i] <> Null
;						If Ar\Instances[j]\OwnedScenery[i]\AccountName$ <> ""
;							SaveInstance = True
;							Exit
;						Else
;							For k = 0 To Ar\Instances[j]\OwnedScenery[i]\InventorySize - 1
;								If Ar\Instances[j]\OwnedScenery[i]\Inventory\Items[k] <> Null
;									SaveInstance = True
;									Exit
;								EndIf
;							Next
;						EndIf
;					EndIf
;				Next
;			EndIf
;		EndIf
;
;		; Save ownerships for this instance
;		If SaveInstance = True
;			F = WriteFile("Data\Server Data\Areas\Ownerships\" + Ar\Name$ + " (" + Ar\Instances[j]\ID + ") Ownerships.dat")
;			If F = 0 Then RuntimeError("Could not write to " + "Data\Server Data\Areas\Ownerships\" + Ar\Name$ + " (" + Ar\Instances[j]\ID + ") Ownerships.dat!")
;
;				For i = 0 To 499
;					If Ar\Instances[j]\OwnedScenery[i] <> Null
;						WriteByte(F, 1)
;						WriteString(F, Ar\Instances[j]\OwnedScenery[i]\AccountName$)
;						WriteByte(F, Ar\Instances[j]\OwnedScenery[i]\CharNumber)
;						WriteByte(F, Ar\Instances[j]\OwnedScenery[i]\InventorySize)
;						If Ar\Instances[j]\OwnedScenery[i]\Inventory <> Null
;							For k = 0 To Ar\Instances[j]\OwnedScenery[i]\InventorySize - 1
;								WriteItemInstance(F, Ar\Instances[j]\OwnedScenery[i]\Inventory\Items[k])
;								WriteShort(F, Ar\Instances[j]\OwnedScenery[i]\Inventory\Amounts[k])
;							Next
;						EndIf
;					Else
;						WriteByte(F, 0)
;					EndIf
;				Next
;
;			CloseFile(F)
;		EndIf
;;	Next
;
;End Function