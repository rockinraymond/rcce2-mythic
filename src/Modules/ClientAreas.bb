; ClientAreas.bb -- GUE-side area UI + editor save path. The zone-load DATA
; path (area types, LoadAreaData, UnloadArea, SetViewDistance, ChunkTerrain)
; lives in Modules\AreaLoader.bb since ADR-004 Phase B. This file keeps:
;   - GUE's Gooey implementations of the AreaLoad* hooks,
;   - the signature-compatible LoadArea wrapper,
;   - SaveArea (editor-only write path).
; AreaLoader.bb must be Included before this file (GUE.bb does).

; Loading-screen state shared between the AreaLoad* hooks. These were locals
; of the pre-carve LoadArea; AreaLoadBegin resets them on every load to keep
; the fresh-locals-per-call semantics (a stale AreaLoadScreen from a previous
; load would otherwise reach GY_UpdateProgressBar after being freed).
Global AreaLoadProgressBar, AreaLoadScreen
Global AreaLoadPMusic, AreaLoadCMusic

; UI hook called by LoadAreaData (Modules\AreaLoader.bb) right after the
; loading-screen texture/music IDs are read from the area file: starts the
; loading music and (when not in display-items mode) builds the progress bar
; and the loading-screen quad. Body moved verbatim from the pre-carve
; LoadArea, with the four locals promoted to the AreaLoad* globals above.
Function AreaLoadBegin(DisplayItems)

	AreaLoadProgressBar = 0
	AreaLoadScreen = 0
	AreaLoadPMusic = 0
	AreaLoadCMusic = 0

		; Music
		If LoadingMusicID < 65535 Then 
			AreaLoadPMusic = LoadSound("Data\Music\" + GetMusicName$(LoadingMusicID), False)
			LoopSound AreaLoadPMusic
			AreaLoadCMusic = PlaySound(AreaLoadPMusic)
		EndIf		
		If DisplayItems = False
			; Progress bar
			AreaLoadProgressBar = GY_CreateProgressBar(0, 0.3, 0.9, 0.4, 0.035, 0, 100, 255, 255, 255, -3012)
			; Preset image
			AreaLoadScreen = CreateMesh(GY_Cam)
			Surf = CreateSurface(AreaLoadScreen)
			v1 = AddVertex(Surf, 0.0, -1.0, 0.0, 0.0, 1.0)
			v2 = AddVertex(Surf, 1.0, -1.0, 0.0, 1.0, 1.0)
			v3 = AddVertex(Surf, 1.0, 0.0, 0.0, 1.0, 0.0)
			v4 = AddVertex(Surf, 0.0, 0.0, 0.0, 0.0, 0.0)
			AddTriangle Surf, v3, v2, v1
			AddTriangle Surf, v4, v3, v1
			
			
			;Widescreen Ramoida
			If ResolutionType = 1 ; 16:9 ratio
				ScaleMesh AreaLoadScreen, 27.0, 15.05, 1.0 ;x,y,z
				PositionEntity AreaLoadScreen, -13.5, 7.55, 10.0
			Else  ;;4:3 ratio
				ScaleMesh AreaLoadScreen, 20.5, 15.5, 1.0
				PositionEntity AreaLoadScreen, -10.07, 7.55, 10.0
			EndIf
			
			EntityOrder AreaLoadScreen,-3011
			EntityFX AreaLoadScreen, 1 + 8
						
			If LoadingTexID < 65535
				Tex = GetTexture(LoadingTexID)
				If Tex <> 0
					EntityTexture(AreaLoadScreen, Tex)
					UnloadTexture(LoadingTexID)
				EndIf
			; Random image
			ElseIf RandomImages > 0
				D = ReadDir("Data\Textures\Random")
				If D = 0
					EntityColor(AreaLoadScreen, 0, 0, 0)
				Else
					For i = 1 To Rand(1, RandomImages)
						Repeat
							File$ = NextFile$(D)
						Until FileType("Data\Textures\Random\" + File$) = 1 Or File$ = ""
						If File$ = "" Then Exit
					Next
					If FileType("Data\Textures\Random\" + File$) = 1
						Tex = LoadTexture("Data\Textures\Random\" + File$)
						If Tex = 0
							EntityColor(AreaLoadScreen, 0, 0, 0)
						Else
							EntityTexture(AreaLoadScreen, Tex)
							FreeTexture(Tex)
						EndIf
					Else
						EntityColor(AreaLoadScreen, 0, 0, 0)
					EndIf
					CloseDir(D)
				EndIf
			; No image
			Else
				EntityColor(AreaLoadScreen, 0, 0, 0)
			EndIf
		EndIf

End Function

; UI hook: progress-bar milestone. The AreaLoadScreen gate replicates the
; pre-carve `If LoadScreen <> 0` check around every update site.
Function AreaLoadProgress(Pct)

	If AreaLoadScreen <> 0
		GY_UpdateProgressBar(AreaLoadProgressBar, Pct)
		RenderWorld()
		Flip()
	EndIf

End Function

; UI hook: tear down the loading screen and stop the loading music.
Function AreaLoadEnd()

	; End loading screen
	If AreaLoadScreen <> 0
		FreeEntity(AreaLoadScreen)
		;FreeEntity(LoadLabel)
		GY_FreeGadget(AreaLoadProgressBar)
	EndIf
	If ChannelPlaying(AreaLoadCMusic) = True Then StopChannel(AreaLoadCMusic)
	FreeSound AreaLoadPMusic

	AreaLoadProgressBar = 0
	AreaLoadScreen = 0
	AreaLoadPMusic = 0
	AreaLoadCMusic = 0

End Function

; Loads the client (3D) data for an area. Thin wrapper kept so existing GUE
; call sites are untouched -- the data path is LoadAreaData (AreaLoader.bb),
; and the loading-screen presentation comes back in via the hooks above.
Function LoadArea(Name$, CameraEN, DisplayItems = False, UpdateRottNet = False)

	Return LoadAreaData(Name$, CameraEN, DisplayItems, UpdateRottNet)

End Function

; Saves the current area back to file. Atomic rewrite via SafeWrite:
; a crash mid-flush previously truncated the area file and broke load
; on the next area entry.
Function SaveArea(Name$)

	Local FinalPath$ = "Data\Areas\" + Name$ + ".dat"
	Local TempPath$ = SafeWriteOpen(FinalPath$)
	F = WriteFile(TempPath$)
	If F = 0 Then Return False

		; Loading screen
		WriteShort F, LoadingTexID
		WriteShort F, LoadingMusicID

		; Environment
		WriteShort F, SkyTexID
		WriteShort F, CloudTexID
		WriteShort F, StormCloudTexID
		WriteShort F, StarsTexID

		WriteByte F, FogR
		WriteByte F, FogG
		WriteByte F, FogB
		WriteFloat F, FogNear#
		WriteFloat F, FogFar#

		WriteShort F, MapTexID
		WriteByte F, Outdoors
		WriteByte F, AmbientR
		WriteByte F, AmbientG
		WriteByte F, AmbientB
		WriteFloat F, DefaultLightPitch#
		WriteFloat F, DefaultLightYaw#
		WriteFloat F, SlopeRestrict#

		; Scenery
		Count = 0
		For S.Scenery = Each Scenery : Count = Count + 1 : Next
		WriteShort F, Count
		For S.Scenery = Each Scenery
			WriteShort F, S\MeshID
			WriteFloat F, EntityX#(S\EN, True)
			WriteFloat F, EntityY#(S\EN, True)
			WriteFloat F, EntityZ#(S\EN, True)
			WriteFloat F, EntityPitch#(S\EN, True)
			WriteFloat F, EntityYaw#(S\EN, True)
			WriteFloat F, EntityRoll#(S\EN, True)
			WriteFloat F, S\ScaleX#
			WriteFloat F, S\ScaleY#
			WriteFloat F, S\ScaleZ#
			WriteByte F, S\AnimationMode
			WriteByte F, S\SceneryID
			WriteShort F, S\TextureID
			WriteByte F, S\CatchRain
						
			WriteByte F, GetEntityType(S\EN)
			WriteString F, S\Lightmap$
			WriteString F, S\RCTE$ ; Extra data for RTCE
			
			WriteByte F, S\CastShadow ;[010]
			WriteByte F, S\ReceiveShadow
			WriteByte F, S\RenderRange ;[011]
			
		Next

		; Water
		Count = 0
		For W.Water = Each Water : Count = Count + 1 : Next
		WriteShort F, Count
		For W.Water = Each Water
			WriteShort F, W\TexID
			WriteFloat F, W\TexScale#
			WriteFloat F, EntityX#(W\EN, True)
			WriteFloat F, EntityY#(W\EN, True)
			WriteFloat F, EntityZ#(W\EN, True)
			WriteFloat F, W\ScaleX#
			WriteFloat F, W\ScaleZ#
			WriteByte F, W\Red
			WriteByte F, W\Green
			WriteByte F, W\Blue
			WriteByte F, W\Opacity
		Next

		; Collision boxes
		Count = 0
		For C.ColBox = Each ColBox : Count = Count + 1 : Next
		WriteShort F, Count
		For C.ColBox = Each ColBox
			WriteFloat F, EntityX#(C\EN, True)
			WriteFloat F, EntityY#(C\EN, True)
			WriteFloat F, EntityZ#(C\EN, True)
			WriteFloat F, EntityPitch#(C\EN, True)
			WriteFloat F, EntityYaw#(C\EN, True)
			WriteFloat F, EntityRoll#(C\EN, True)
			WriteFloat F, C\ScaleX#
			WriteFloat F, C\ScaleY#
			WriteFloat F, C\ScaleZ#
		Next

		; Emitters
		Count = 0
		For E.Emitter = Each Emitter : Count = Count + 1 : Next
		WriteShort F, Count
		For E.Emitter = Each Emitter
			WriteString F, E\ConfigName$
			WriteShort F, E\TexID
			WriteFloat F, EntityX#(E\EN, True)
			WriteFloat F, EntityY#(E\EN, True)
			WriteFloat F, EntityZ#(E\EN, True)
			WriteFloat F, EntityPitch#(E\EN, True)
			WriteFloat F, EntityYaw#(E\EN, True)
			WriteFloat F, EntityRoll#(E\EN, True)
		Next

		; Terrains
		Count = 0
		For T.Terrain = Each Terrain :  Count = Count + 1 : Next
		WriteShort F, Count
		For T.Terrain = Each Terrain
			WriteShort F, T\BaseTexID
			WriteShort F, T\DetailTexID
			WriteInt F, TerrainSize(T\EN)
			For X = 0 To TerrainSize(T\EN)
				For Z = 0 To TerrainSize(T\EN)
					WriteFloat F, TerrainHeight#(T\EN, X, Z)
				Next
			Next
			WriteFloat F, EntityX#(T\EN, True)
			WriteFloat F, EntityY#(T\EN, True)
			WriteFloat F, EntityZ#(T\EN, True)
			WriteFloat F, EntityPitch#(T\EN, True)
			WriteFloat F, EntityYaw#(T\EN, True)
			WriteFloat F, EntityRoll#(T\EN, True)
			WriteFloat F, T\ScaleX#
			WriteFloat F, T\ScaleY#
			WriteFloat F, T\ScaleZ#
			WriteFloat F, T\DetailTexScale#
			WriteInt   F, T\Detail
			WriteByte  F, T\Morph
			WriteByte  F, T\Shading
		Next

		; Sound zones
		Count = 0
		For SZ.SoundZone = Each SoundZone : Count = Count + 1 : Next
		WriteShort F, Count
		For SZ.SoundZone = Each SoundZone
			WriteFloat F, EntityX#(SZ\EN, True)
			WriteFloat F, EntityY#(SZ\EN, True)
			WriteFloat F, EntityZ#(SZ\EN, True)
			WriteFloat F, SZ\Radius#
			WriteShort F, SZ\SoundID
			WriteShort F, SZ\MusicID
			WriteInt F, SZ\RepeatTime
			WriteByte F, SZ\Volume
		Next

	Return SafeWriteCommit(TempPath$, FinalPath$, F)

End Function
