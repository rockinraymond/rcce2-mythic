; =============================================================================
; Loom/ZoneViewport.bb -- schematic 3D viewport for zone sub-entities
; =============================================================================
;
; The "real" 3D zone viewport (with terrain meshes, scenery, water, sky) is
; still deferred per ADR 004 -- ClientAreas.bb's LoadArea is entangled with
; GUE-specific UI globals.  But Loom already has every PORTAL / SPAWN /
; TRIGGER / WAYPOINT coordinate in memory via ServerLoadArea (the
; data-only loader).  That's enough for a SCHEMATIC viewport that shows
; the zone's topology: where portals are, where spawns cluster, where
; triggers fire, how waypoints connect.
;
; Visual:
;   Ground plane           dark stone-900 quad at y=0
;   Portal markers         brass cubes
;   Spawn markers          arcane-cyan cubes
;   Trigger markers        warning-orange cubes
;   Waypoint markers       small stone cubes + lines for prev/next chain
;
; Camera:
;   Same isolated-y trick as MeshPreview -- viewport camera lives at
;   y=20000 (far from mesh preview at 10000 and any future world).
;   The zone's own data sits at ground level (y~=0) but we OFFSET the
;   entire schematic scene up by VP_SCENE_Y_OFFSET so the viewport
;   camera can find it without conflicting with other previews.
;
; Mouse:
;   LMB drag = orbit around scene center
;   wheel    = zoom in/out
;   (Pan + click-to-focus-marker are follow-up iters.)
;
; Performance:
;   Markers are CreateCube'd once per visible sub-entity. Reset on zone
;   change. A zone with 100 portals + 1000 spawns + 150 triggers + 2000
;   waypoints = ~3250 entities -- well within Blitz3D's headroom.
;
; Non-Strict (matches MeshPreview / Settings / Recents).


Const VP_RT_SIZE          = 384
Const VP_SCENE_Y_OFFSET#  = 20000.0      ; isolate from mesh preview at y=10000
Const VP_DEFAULT_CAM_DIST# = 400.0
Const VP_MARKER_SIZE#     = 4.0
Const VP_WAYPOINT_SIZE#   = 1.5


; ---- Module state -----------------------------------------------------------
Global VPCam        = 0
Global VPLight      = 0
Global VPRT         = 0
Global VPGround     = 0           ; ground plane entity
Global VPInitOK     = False
Global VPLoadedZoneH = 0          ; Handle(Area) of currently-loaded zone

; Camera orbit state
Global VPYaw#       = 0.0
Global VPPitch#     = 25.0
Global VPDistance#  = VP_DEFAULT_CAM_DIST#
Global VPSceneCenterX# = 0.0     ; auto-fit center of the loaded zone
Global VPSceneCenterY# = 0.0
Global VPSceneCenterZ# = 0.0
Global VPDragging   = False
Global VPLastMX     = 0
Global VPLastMY     = 0


; =============================================================================
; Loom_InitZoneViewport -- one-time setup at boot.
; =============================================================================
Function Loom_InitZoneViewport()
    If VPInitOK = True Then Return

    VPRT = CreateTexture(VP_RT_SIZE, VP_RT_SIZE, 1 + 256)
    If VPRT = 0
        WriteLog(LoomLog, "ZoneViewport: CreateTexture failed -- viewport disabled")
        Return
    EndIf
    TextureBlend VPRT, 0

    VPCam = CreateCamera()
    PositionEntity VPCam, 0, VP_SCENE_Y_OFFSET# + 100, -VP_DEFAULT_CAM_DIST#
    PointEntity VPCam, 0
    CameraClsColor VPCam, 16, 16, 22
    CameraRange    VPCam, 1.0, 10000.0
    HideEntity     VPCam

    VPLight = CreateLight(1)
    PositionEntity VPLight, 0, VP_SCENE_Y_OFFSET# + 500, -200
    RotateEntity   VPLight, 60, -45, 0
    LightColor     VPLight, 255, 255, 230

    ; Ground plane -- a large flat cube acting as the zone floor.
    ; CreateCube returns a unit cube; scale to a wide flat slab.
    VPGround = CreateCube()
    ScaleEntity VPGround, 800.0, 0.5, 800.0
    PositionEntity VPGround, 0, VP_SCENE_Y_OFFSET#, 0
    EntityColor VPGround, 24, 24, 32      ; near-black stone

    VPInitOK = True
    WriteLog(LoomLog, "ZoneViewport: initialized (RT=" + VP_RT_SIZE + "x" + VP_RT_SIZE + ")")
End Function


; =============================================================================
; Loom_FreeZoneMarkers -- free every per-sub-entity marker entity. Called
; on zone change and at shutdown. Uses a marker-only collection via the
; ZoneViewportMarker type so we don't accidentally free the camera/ground/
; light.
; =============================================================================
Type ZoneViewportMarker
    Field EN
End Type

Function Loom_FreeZoneMarkers()
    Local m.ZoneViewportMarker
    For m = Each ZoneViewportMarker
        If m\EN <> 0 Then FreeEntity m\EN
    Next
    For m = Each ZoneViewportMarker
        Delete m
    Next
End Function


; =============================================================================
; Loom_LoadZoneMarkers -- walk the Area's portal/spawn/trigger/waypoint
; arrays, instantiate a colored cube for each defined entry. Also computes
; the scene bbox so the camera can auto-fit.
; =============================================================================
Function Loom_LoadZoneMarkers(Ar.Area)
    Loom_FreeZoneMarkers()
    If Ar = Null Then Return

    Local minX# = 1000000.0
    Local minZ# = 1000000.0
    Local maxX# = -1000000.0
    Local maxZ# = -1000000.0
    Local found = False

    ; Portals -- brass cubes
    Local i
    For i = 0 To 99
        If Ar\PortalName$[i] <> ""
            Local pEn = CreateCube()
            ScaleEntity pEn, VP_MARKER_SIZE#, VP_MARKER_SIZE#, VP_MARKER_SIZE#
            PositionEntity pEn, Ar\PortalX#[i], VP_SCENE_Y_OFFSET# + Ar\PortalY#[i] + VP_MARKER_SIZE#, Ar\PortalZ#[i]
            EntityColor pEn, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B
            Local pm.ZoneViewportMarker = New ZoneViewportMarker
            pm\EN = pEn
            If Ar\PortalX#[i] < minX# Then minX# = Ar\PortalX#[i]
            If Ar\PortalX#[i] > maxX# Then maxX# = Ar\PortalX#[i]
            If Ar\PortalZ#[i] < minZ# Then minZ# = Ar\PortalZ#[i]
            If Ar\PortalZ#[i] > maxZ# Then maxZ# = Ar\PortalZ#[i]
            found = True
        EndIf
    Next

    ; Spawns -- arcane cubes
    For i = 0 To 999
        If Ar\SpawnActor[i] > 0
            Local waypointIdx = Ar\SpawnWaypoint[i]
            If waypointIdx >= 0 And waypointIdx <= 1999
                Local sEn = CreateCube()
                ScaleEntity sEn, VP_MARKER_SIZE#, VP_MARKER_SIZE#, VP_MARKER_SIZE#
                PositionEntity sEn, Ar\WaypointX#[waypointIdx], VP_SCENE_Y_OFFSET# + Ar\WaypointY#[waypointIdx] + VP_MARKER_SIZE#, Ar\WaypointZ#[waypointIdx]
                EntityColor sEn, LOOM_ARCANE_500_R, LOOM_ARCANE_500_G, LOOM_ARCANE_500_B
                Local sm.ZoneViewportMarker = New ZoneViewportMarker
                sm\EN = sEn
                If Ar\WaypointX#[waypointIdx] < minX# Then minX# = Ar\WaypointX#[waypointIdx]
                If Ar\WaypointX#[waypointIdx] > maxX# Then maxX# = Ar\WaypointX#[waypointIdx]
                If Ar\WaypointZ#[waypointIdx] < minZ# Then minZ# = Ar\WaypointZ#[waypointIdx]
                If Ar\WaypointZ#[waypointIdx] > maxZ# Then maxZ# = Ar\WaypointZ#[waypointIdx]
                found = True
            EndIf
        EndIf
    Next

    ; Triggers -- warning cubes
    For i = 0 To 149
        If Ar\TriggerScript$[i] <> ""
            Local tEn = CreateCube()
            ScaleEntity tEn, VP_MARKER_SIZE#, VP_MARKER_SIZE#, VP_MARKER_SIZE#
            PositionEntity tEn, Ar\TriggerX#[i], VP_SCENE_Y_OFFSET# + Ar\TriggerY#[i] + VP_MARKER_SIZE#, Ar\TriggerZ#[i]
            EntityColor tEn, LOOM_WARNING_R, LOOM_WARNING_G, LOOM_WARNING_B
            Local tm.ZoneViewportMarker = New ZoneViewportMarker
            tm\EN = tEn
            If Ar\TriggerX#[i] < minX# Then minX# = Ar\TriggerX#[i]
            If Ar\TriggerX#[i] > maxX# Then maxX# = Ar\TriggerX#[i]
            If Ar\TriggerZ#[i] < minZ# Then minZ# = Ar\TriggerZ#[i]
            If Ar\TriggerZ#[i] > maxZ# Then maxZ# = Ar\TriggerZ#[i]
            found = True
        EndIf
    Next

    ; Waypoints -- small stone cubes (only render defined ones)
    For i = 0 To 1999
        If Ar\WaypointX#[i] <> 0.0 Or Ar\WaypointZ#[i] <> 0.0
            Local wEn = CreateCube()
            ScaleEntity wEn, VP_WAYPOINT_SIZE#, VP_WAYPOINT_SIZE#, VP_WAYPOINT_SIZE#
            PositionEntity wEn, Ar\WaypointX#[i], VP_SCENE_Y_OFFSET# + Ar\WaypointY#[i] + VP_WAYPOINT_SIZE#, Ar\WaypointZ#[i]
            EntityColor wEn, 140, 140, 150
            Local wm.ZoneViewportMarker = New ZoneViewportMarker
            wm\EN = wEn
            If Ar\WaypointX#[i] < minX# Then minX# = Ar\WaypointX#[i]
            If Ar\WaypointX#[i] > maxX# Then maxX# = Ar\WaypointX#[i]
            If Ar\WaypointZ#[i] < minZ# Then minZ# = Ar\WaypointZ#[i]
            If Ar\WaypointZ#[i] > maxZ# Then maxZ# = Ar\WaypointZ#[i]
            found = True
        EndIf
    Next

    ; Auto-fit camera: center on midpoint of bbox, distance scaled
    ; to the larger of the two extents.
    If found = True
        VPSceneCenterX# = (minX# + maxX#) / 2.0
        VPSceneCenterY# = 0.0
        VPSceneCenterZ# = (minZ# + maxZ#) / 2.0
        Local extentX# = maxX# - minX#
        Local extentZ# = maxZ# - minZ#
        Local extent# = extentX#
        If extentZ# > extent# Then extent# = extentZ#
        If extent# < 100.0 Then extent# = 100.0
        VPDistance# = extent# * 1.5
    Else
        VPSceneCenterX# = 0.0
        VPSceneCenterY# = 0.0
        VPSceneCenterZ# = 0.0
        VPDistance# = VP_DEFAULT_CAM_DIST#
    EndIf

    ; Reset orbit so each new zone starts at a comfortable default angle.
    VPYaw# = 0.0
    VPPitch# = 25.0
End Function


; =============================================================================
; Loom_DrawZoneViewport -- public render entry. Lazy-loads markers for the
; zone if the zone handle changed since last frame. Then handles orbit/
; zoom input, repositions the camera, renders to RT, blits to back buffer.
; =============================================================================
Function Loom_DrawZoneViewport(zoneHandle, x, y)
    If VPInitOK = False
        LoomFill x, y, VP_RT_SIZE, VP_RT_SIZE, LOOM_STONE_900_R, LOOM_STONE_900_G, LOOM_STONE_900_B
        LoomBorder x, y, VP_RT_SIZE, VP_RT_SIZE, LOOM_STONE_700_R, LOOM_STONE_700_G, LOOM_STONE_700_B
        LoomText x + 8, y + 8, "viewport init failed", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B
        Return False
    EndIf

    Local Ar.Area = Object.Area(zoneHandle)
    If Ar = Null
        LoomFill x, y, VP_RT_SIZE, VP_RT_SIZE, LOOM_STONE_900_R, LOOM_STONE_900_G, LOOM_STONE_900_B
        LoomBorder x, y, VP_RT_SIZE, VP_RT_SIZE, LOOM_STONE_700_R, LOOM_STONE_700_G, LOOM_STONE_700_B
        LoomText x + 8, y + 8, "no zone focused", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B
        Return False
    EndIf

    If zoneHandle <> VPLoadedZoneH
        Loom_LoadZoneMarkers(Ar)
        VPLoadedZoneH = zoneHandle
        WriteLog(LoomLog, "ZoneViewport: loaded zone " + Ar\Name$)
    EndIf

    ; ---- Input handling -----------------------------------------------------
    Local mx = MouseX()
    Local my = MouseY()
    Local inside = (mx >= x And mx < x + VP_RT_SIZE And my >= y And my < y + VP_RT_SIZE)

    If MouseDown(1) = True And inside = True
        If VPDragging = False
            VPDragging = True
            VPLastMX = mx
            VPLastMY = my
        Else
            Local dx = mx - VPLastMX
            Local dy = my - VPLastMY
            VPYaw# = VPYaw# + Float(dx) * 0.5
            VPPitch# = VPPitch# + Float(dy) * 0.5
            If VPPitch# > 89.0 Then VPPitch# = 89.0
            If VPPitch# < -89.0 Then VPPitch# = -89.0
            VPLastMX = mx
            VPLastMY = my
        EndIf
    Else
        VPDragging = False
    EndIf

    If inside = True
        Local wheel = Loom_MouseWheel()
        If wheel <> 0
            VPDistance# = VPDistance# - Float(wheel) * (VPDistance# * 0.08)
            If VPDistance# < 20.0 Then VPDistance# = 20.0
            If VPDistance# > 5000.0 Then VPDistance# = 5000.0
        EndIf
    EndIf

    ; ---- Position camera by orbit math -------------------------------------
    Local yawRad# = VPYaw# * 3.14159 / 180.0
    Local pitchRad# = VPPitch# * 3.14159 / 180.0
    Local cx# = VPSceneCenterX# + Cos(VPPitch#) * Sin(VPYaw#) * VPDistance#
    Local cy# = VP_SCENE_Y_OFFSET# + VPSceneCenterY# + Sin(VPPitch#) * VPDistance#
    Local cz# = VPSceneCenterZ# - Cos(VPPitch#) * Cos(VPYaw#) * VPDistance#
    PositionEntity VPCam, cx#, cy#, cz#
    PointEntity VPCam, VPGround   ; PointEntity at ground center keeps camera level

    ; ---- Render -------------------------------------------------------------
    ShowEntity VPCam
    SetBuffer TextureBuffer(VPRT)
    RenderWorld
    SetBuffer BackBuffer()
    HideEntity VPCam

    ; Blit to back buffer
    CopyRect 0, 0, VP_RT_SIZE, VP_RT_SIZE, x, y, TextureBuffer(VPRT), BackBuffer()
    LoomBorder x, y, VP_RT_SIZE, VP_RT_SIZE, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B

    ; Legend overlay
    LoomText x + 8, y + 8,  "portals",   LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B
    LoomText x + 8, y + 24, "spawns",    LOOM_ARCANE_500_R, LOOM_ARCANE_500_G, LOOM_ARCANE_500_B
    LoomText x + 8, y + 40, "triggers",  LOOM_WARNING_R, LOOM_WARNING_G, LOOM_WARNING_B
    LoomText x + 8, y + 56, "waypoints", 200, 200, 210

    If inside = True
        LoomText x + 8, y + VP_RT_SIZE - 18, "drag=orbit  wheel=zoom", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B
    EndIf

    Return True
End Function


; =============================================================================
; Loom_ShutdownZoneViewport -- free GPU resources at exit.
; =============================================================================
Function Loom_ShutdownZoneViewport()
    Loom_FreeZoneMarkers()
    If VPGround <> 0 Then FreeEntity VPGround : VPGround = 0
    If VPCam <> 0 Then FreeEntity VPCam : VPCam = 0
    If VPLight <> 0 Then FreeEntity VPLight : VPLight = 0
    If VPRT <> 0 Then FreeTexture VPRT : VPRT = 0
    VPInitOK = False
End Function
