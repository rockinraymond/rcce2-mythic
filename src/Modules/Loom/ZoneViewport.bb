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


; Sized to fit inside the 380px composer panel (CMP_W) with the
; standard CMP_PAD breathing room. Previously 384 which leaked
; left of the panel and produced the overlapping-text bug a user
; reported (composer fields and viewport overlay both drew in the
; same X column).
Const VP_RT_SIZE          = 320
Const VP_SCENE_Y_OFFSET#  = 20000.0      ; isolate from mesh preview at y=10000
Const VP_DEFAULT_CAM_DIST# = 400.0
Const VP_MARKER_SIZE#     = 4.0
Const VP_WAYPOINT_SIZE#   = 1.5
Const VP_AXIS_LENGTH#     = 30.0          ; XYZ axis indicator length
Const VP_AXIS_THICKNESS#  = 0.3           ; thin cube acting as a line
Const VP_LINE_THICKNESS#  = 0.25          ; waypoint connection line thickness
Const VP_MAX_LINES        = 500           ; cap connection line entities


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
Global VPDragStartMX = 0
Global VPDragStartMY = 0

; Marker drag-to-edit state. Right-click on a marker to enter drag
; mode; subsequent frames track the cursor on the ground plane and
; reposition the marker + update the underlying Area coord field.
; Release RMB to commit.
Global VPMarkerDragging   = False
Global VPMarkerDragEN     = 0            ; entity handle of the marker being dragged
Global VPMarkerDragKind$  = ""
Global VPMarkerDragIdx    = -1
Global VPMarkerDragArH    = 0            ; Handle(Area) of the zone being edited

; MMB pan state. Middle-mouse drag translates VPSceneCenterX/Z in
; camera-aligned screen-right and screen-forward directions so the
; user can scroll the view to focus on a particular zone corner.
Global VPPanning   = False
Global VPPanLastMX = 0
Global VPPanLastMY = 0

; Per-zone counts cached at load time (saves recomputing in renderer
; just for the legend overlay).
Global VPCountPortals  = 0
Global VPCountSpawns   = 0
Global VPCountTriggers = 0
Global VPCountWaypoints = 0
Global VPCountLines    = 0     ; total connection lines emitted

; Render-on-change flag. Set True any time camera / markers / highlight
; change; consumed by Loom_DrawZoneViewport which only does
; RenderWorld + CopyRect when True. Cached pixels persist between
; frames so a static viewport costs ~0.
Global VPDirty         = True

; Last-frame highlight so we only ScaleEntity on TRANSITIONS instead
; of every frame's full marker walk. Empty/-1 = no previous highlight.
Global VPPrevHighlightKind$ = ""
Global VPPrevHighlightIdx   = -1

; Module-level Composer pointer set by Loom.bb after construction.
; Lets Loom_PickZoneMarker dispatch into Composer::scrollToZoneSubEntity
; without holding a per-call reference. Same shape as LoomWorldCache.
Global LoomComposer.Composer = Null

; LoomZoneHighlightKind$ / LoomZoneHighlightIdx live in ImageCache.bb
; (included BEFORE Composer) so the Strict Composer module can write
; them without the dim-write-from-Strict trap that bit Settings
; globals. See ImageCache.bb / feedback_loom_module_include_order.


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
    ; Constrain the camera's viewport to the render-target size so
    ; CameraPick(cam, x, y) treats (x, y) as 0..VP_RT_SIZE coords
    ; (which lets us pass mouse coords RELATIVE to the widget rect).
    CameraViewport VPCam, 0, 0, VP_RT_SIZE, VP_RT_SIZE
    HideEntity     VPCam

    VPLight = CreateLight(1)
    PositionEntity VPLight, 0, VP_SCENE_Y_OFFSET# + 500, -200
    RotateEntity   VPLight, 60, -45, 0
    LightColor     VPLight, 255, 255, 230

    ; Ground plane -- a large flat cube acting as the zone floor.
    ; CreateCube returns a unit cube; scale to a wide flat slab.
    ; Scale is huge so drag-to-edit works for big zones (cursor
    ; can fall outside a "normal" zone's bbox during a fast drag).
    ; EntityPickMode = 2 (poly pick) so CameraPick can land on the
    ; ground plane during a drag and return its world position.
    VPGround = CreateCube()
    ScaleEntity VPGround, 5000.0, 0.5, 5000.0
    PositionEntity VPGround, 0, VP_SCENE_Y_OFFSET#, 0
    EntityColor VPGround, 24, 24, 32      ; near-black stone
    EntityPickMode VPGround, 2            ; polygon pick (for ground drag-land)

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
    Field Kind$        ; "portal" / "spawn" / "trigger" / "waypoint" / "" for axis/line decorations
    Field IndexN%      ; sub-entity slot index inside the zone (0..N-1 per kind)
    Field BaseScale#   ; uniform scale applied at marker creation; used by
                       ; highlight system to restore size when un-highlighted
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
; Loom_MakeLine -- emit a thin scaled cube positioned + oriented as the
; segment from (x1, y1, z1) to (x2, y2, z2). Blitz3D has no native 3D
; line primitive; this is the cheap stand-in.
;
; Uses the trig form: midpoint + length + atan2(dx, dz) for yaw, then
; tilt for pitch via atan2(dy, horiz_len). Z axis is the cube's
; "length" direction; we scale Z to the segment length and X/Y to
; VP_LINE_THICKNESS.
; =============================================================================
Function Loom_MakeLine(x1#, y1#, z1#, x2#, y2#, z2#, r, g, b)
    If VPCountLines >= VP_MAX_LINES Then Return

    Local dx# = x2# - x1#
    Local dy# = y2# - y1#
    Local dz# = z2# - z1#
    Local len# = Sqr(dx# * dx# + dy# * dy# + dz# * dz#)
    If len# < 0.01 Then Return

    Local en = CreateCube()
    ScaleEntity en, VP_LINE_THICKNESS#, VP_LINE_THICKNESS#, len# / 2.0
    EntityColor en, r, g, b
    PositionEntity en, (x1# + x2#) / 2.0, (y1# + y2#) / 2.0, (z1# + z2#) / 2.0

    ; Orient: yaw around Y (atan2 of horiz), pitch around X (atan2 of dy/horiz)
    Local horiz# = Sqr(dx# * dx# + dz# * dz#)
    Local yaw#   = ATan2(dx#, dz#)
    Local pitch# = -ATan2(dy#, horiz#)
    RotateEntity en, pitch#, yaw#, 0

    Local marker.ZoneViewportMarker = New ZoneViewportMarker
    marker\EN = en
    VPCountLines = VPCountLines + 1
End Function


; =============================================================================
; Loom_MakeAxisMarkers -- three short colored lines from the scene origin
; along +X (red), +Y (green), +Z (blue). Gives the viewport an obvious
; orientation reference at scene origin.
; =============================================================================
Function Loom_MakeAxisMarkers()
    Local ox# = 0.0
    Local oy# = VP_SCENE_Y_OFFSET#
    Local oz# = 0.0
    Loom_MakeLine ox#, oy#, oz#, ox# + VP_AXIS_LENGTH#, oy#, oz#, 220, 60, 60
    Loom_MakeLine ox#, oy#, oz#, ox#, oy# + VP_AXIS_LENGTH#, oz#, 60, 220, 60
    Loom_MakeLine ox#, oy#, oz#, ox#, oy#, oz# + VP_AXIS_LENGTH#, 60, 120, 220
End Function


; =============================================================================
; Loom_CommitMarkerCoord -- write the new X/Z (and keep current Y) back
; to the underlying Area field for the dragged sub-entity. Called every
; frame during drag for live preview. ZoneSaved gets flipped to False
; via Composer::markDirtyForKind on release.
; =============================================================================
Function Loom_CommitMarkerCoord(zoneHandle, kind$, idx, newX#, newZ#)
    Local Ar.Area = Object.Area(zoneHandle)
    If Ar = Null Then Return
    If kind$ = "portal"
        If idx >= 0 And idx <= 99
            Ar\PortalX#[idx] = newX#
            Ar\PortalZ#[idx] = newZ#
        EndIf
    Else If kind$ = "trigger"
        If idx >= 0 And idx <= 149
            Ar\TriggerX#[idx] = newX#
            Ar\TriggerZ#[idx] = newZ#
        EndIf
    Else If kind$ = "spawn"
        If idx >= 0 And idx <= 999
            ; Spawn position is the waypoint position, not a direct
            ; spawn coord. Update the referenced waypoint instead.
            Local wpIdx = Ar\SpawnWaypoint[idx]
            If wpIdx >= 0 And wpIdx <= 1999
                Ar\WaypointX#[wpIdx] = newX#
                Ar\WaypointZ#[wpIdx] = newZ#
            EndIf
        EndIf
    EndIf
End Function


; =============================================================================
; Loom_PickZoneMarker -- cast a ray from the camera through the requested
; local widget coords (0..VP_RT_SIZE), find which marker (if any) was
; hit. On hit, fire a toast naming the sub-entity. Iter 45 will turn
; this into a composer scroll-to-section dispatch.
;
; Note: line cubes (Loom_MakeLine emits these for axes + waypoint
; connections) have EntityPickMode = 0 (the default) so they don't
; intercept the pick ray. Only the marker cubes (portal/spawn/trigger/
; waypoint) are pickable.
; =============================================================================
Function Loom_PickZoneMarker(localX, localY)
    If VPInitOK = False Then Return

    ; CameraPick uses the camera's viewport (set at init to
    ; 0..VP_RT_SIZE), so we pass local widget coords directly.
    CameraPick VPCam, localX, localY
    Local picked = PickedEntity()
    If picked = 0 Then Return

    ; Find which marker has this entity. Linear walk is fine since
    ; total markers stay bounded (typically <50 per zone, capped at
    ; thousands worst-case).
    Local m.ZoneViewportMarker
    For m = Each ZoneViewportMarker
        If m\EN = picked
            If m\Kind <> ""
                ; Dispatch to composer scroll-to-section if available
                ; (waypoint clicks don't have a section to scroll to;
                ; they're rendered inline with other waypoints rather
                ; than as per-slot sub-sections, so we still toast).
                If LoomComposer <> Null And (m\Kind = "portal" Or m\Kind = "trigger" Or m\Kind = "spawn")
                    Local ok = Composer::scrollToZoneSubEntity(LoomComposer, m\Kind, m\IndexN)
                    If ok = True
                        Toast_Show("Jumped to " + m\Kind + " " + Str(m\IndexN), "info")
                    Else
                        Toast_Show("Picked " + m\Kind + " " + Str(m\IndexN) + " (anchor not ready)", "warning")
                    EndIf
                Else
                    Toast_Show("Picked " + m\Kind + " " + Str(m\IndexN), "info")
                EndIf
                WriteLog(LoomLog, "ZoneViewport: picked " + m\Kind + " " + Str(m\IndexN))
            EndIf
            Return
        EndIf
    Next
End Function


; =============================================================================
; Loom_LoadZoneMarkers -- walk the Area's portal/spawn/trigger/waypoint
; arrays, instantiate a colored cube for each defined entry. Also computes
; the scene bbox so the camera can auto-fit.
; =============================================================================
Function Loom_LoadZoneMarkers(Ar.Area)
    Loom_FreeZoneMarkers()
    VPCountPortals  = 0
    VPCountSpawns   = 0
    VPCountTriggers = 0
    VPCountWaypoints = 0
    VPCountLines    = 0
    If Ar = Null Then Return

    ; Origin axis markers go first (cheap, always 3 lines).
    Loom_MakeAxisMarkers()

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
            EntityPickMode pEn, 1     ; box-pick eligible
            Local pm.ZoneViewportMarker = New ZoneViewportMarker
            pm\EN = pEn
            pm\Kind = "portal"
            pm\IndexN = i
            pm\BaseScale = VP_MARKER_SIZE#
            VPCountPortals = VPCountPortals + 1
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
                EntityPickMode sEn, 1
                Local sm.ZoneViewportMarker = New ZoneViewportMarker
                sm\EN = sEn
                sm\Kind = "spawn"
                sm\IndexN = i
                sm\BaseScale = VP_MARKER_SIZE#
                VPCountSpawns = VPCountSpawns + 1
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
            EntityPickMode tEn, 1
            Local tm.ZoneViewportMarker = New ZoneViewportMarker
            tm\EN = tEn
            tm\Kind = "trigger"
            tm\IndexN = i
            tm\BaseScale = VP_MARKER_SIZE#
            VPCountTriggers = VPCountTriggers + 1
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
            EntityPickMode wEn, 1
            Local wm.ZoneViewportMarker = New ZoneViewportMarker
            wm\EN = wEn
            wm\Kind = "waypoint"
            wm\IndexN = i
            wm\BaseScale = VP_WAYPOINT_SIZE#
            VPCountWaypoints = VPCountWaypoints + 1
            If Ar\WaypointX#[i] < minX# Then minX# = Ar\WaypointX#[i]
            If Ar\WaypointX#[i] > maxX# Then maxX# = Ar\WaypointX#[i]
            If Ar\WaypointZ#[i] < minZ# Then minZ# = Ar\WaypointZ#[i]
            If Ar\WaypointZ#[i] > maxZ# Then maxZ# = Ar\WaypointZ#[i]
            found = True
        EndIf
    Next

    ; Waypoint connection lines -- emit a thin line for each NextA/NextB
    ; pointer from a defined waypoint to a defined target. Capped via
    ; VP_MAX_LINES to keep entity count bounded on huge zones.
    For i = 0 To 1999
        If Ar\WaypointX#[i] <> 0.0 Or Ar\WaypointZ#[i] <> 0.0
            Local na = Ar\NextWaypointA[i]
            If na >= 0 And na <= 1999
                If Ar\WaypointX#[na] <> 0.0 Or Ar\WaypointZ#[na] <> 0.0
                    Loom_MakeLine Ar\WaypointX#[i], VP_SCENE_Y_OFFSET# + Ar\WaypointY#[i] + VP_WAYPOINT_SIZE#, Ar\WaypointZ#[i], Ar\WaypointX#[na], VP_SCENE_Y_OFFSET# + Ar\WaypointY#[na] + VP_WAYPOINT_SIZE#, Ar\WaypointZ#[na], 100, 100, 110
                EndIf
            EndIf
            Local nb = Ar\NextWaypointB[i]
            If nb >= 0 And nb <= 1999
                If Ar\WaypointX#[nb] <> 0.0 Or Ar\WaypointZ#[nb] <> 0.0
                    Loom_MakeLine Ar\WaypointX#[i], VP_SCENE_Y_OFFSET# + Ar\WaypointY#[i] + VP_WAYPOINT_SIZE#, Ar\WaypointZ#[i], Ar\WaypointX#[nb], VP_SCENE_Y_OFFSET# + Ar\WaypointY#[nb] + VP_WAYPOINT_SIZE#, Ar\WaypointZ#[nb], 100, 100, 110
                EndIf
            EndIf
        EndIf
    Next

    ; Water rectangles -- walk the per-Area FirstWater linked list
    ; (populated by ServerLoadArea). Render each as a flat blue slab
    ; sized to (Width x small height x Depth) at (X, Y, Z). Damage
    ; type encoded in the color so designers can spot lava (red) vs
    ; acid (green) vs plain water (blue) at a glance.
    Local W.ServerWater = Ar\FirstWater
    While W <> Null
        Local wEn2 = CreateCube()
        ScaleEntity wEn2, W\Width# / 2.0, 1.0, W\Depth# / 2.0
        PositionEntity wEn2, W\X#, VP_SCENE_Y_OFFSET# + W\Y#, W\Z#
        ; Color by damage: 0 = water (blue), 1+ = damage type tinted
        If W\Damage > 0
            EntityColor wEn2, 200, 70, 70    ; harmful = red tint
        Else
            EntityColor wEn2, 70, 110, 200   ; neutral water = blue
        EndIf
        EntityAlpha wEn2, 0.5                 ; translucent so markers below are visible
        Local wm2.ZoneViewportMarker = New ZoneViewportMarker
        wm2\EN = wEn2
        wm2\Kind = ""                        ; not pickable as a sub-entity
        If W\X# - W\Width# / 2.0 < minX# Then minX# = W\X# - W\Width# / 2.0
        If W\X# + W\Width# / 2.0 > maxX# Then maxX# = W\X# + W\Width# / 2.0
        If W\Z# - W\Depth# / 2.0 < minZ# Then minZ# = W\Z# - W\Depth# / 2.0
        If W\Z# + W\Depth# / 2.0 > maxZ# Then maxZ# = W\Z# + W\Depth# / 2.0
        found = True
        W = W\NextWater
    Wend

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
        VPDirty = True
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
            VPDragStartMX = mx     ; remember initial press for click-vs-drag distinguish
            VPDragStartMY = my
        Else
            Local dx = mx - VPLastMX
            Local dy = my - VPLastMY
            If dx <> 0 Or dy <> 0
                VPYaw# = VPYaw# + Float(dx) * 0.5
                VPPitch# = VPPitch# + Float(dy) * 0.5
                If VPPitch# > 89.0 Then VPPitch# = 89.0
                If VPPitch# < -89.0 Then VPPitch# = -89.0
                VPDirty = True
            EndIf
            VPLastMX = mx
            VPLastMY = my
        EndIf
    Else
        ; On LMB release: if the press-to-release total movement was
        ; small (no real drag), treat as a click and try to pick a
        ; marker. Loom_MouseClicked() is False here (it fires on PRESS
        ; not release), so check VPDragging transitioning to False.
        If VPDragging = True
            Local moveDist = Abs(mx - VPDragStartMX) + Abs(my - VPDragStartMY)
            If moveDist < 4 And inside = True
                Loom_PickZoneMarker(mx - x, my - y)
            EndIf
        EndIf
        VPDragging = False
    EndIf

    ; ---- Marker drag-to-edit (RMB) -----------------------------------------
    ; RMB inside viewport hit-tests a marker on press; subsequent frames
    ; track the cursor on the ground plane and update the marker +
    ; underlying Area coord. Release commits.
    If MouseDown(2) = True And inside = True
        If VPMarkerDragging = False
            ; Press: hit-test for a marker. Need to render the scene
            ; first so the camera + entity positions are current for
            ; CameraPick, but we already did that this frame at the
            ; END of the previous renderAndUpdate call. The picks
            ; should still be valid since nothing has moved.
            CameraPick VPCam, mx - x, my - y
            Local pickedEN = PickedEntity()
            If pickedEN <> 0 And pickedEN <> VPGround
                Local pm.ZoneViewportMarker
                For pm = Each ZoneViewportMarker
                    If pm\EN = pickedEN And (pm\Kind = "portal" Or pm\Kind = "trigger" Or pm\Kind = "spawn")
                        VPMarkerDragging = True
                        VPMarkerDragEN   = pickedEN
                        VPMarkerDragKind$ = pm\Kind
                        VPMarkerDragIdx  = pm\IndexN
                        VPMarkerDragArH  = zoneHandle
                        Exit
                    EndIf
                Next
            EndIf
        Else
            ; Drag: re-pick against the ground, hide the dragged marker
            ; first so the ray passes through it.
            HideEntity VPMarkerDragEN
            CameraPick VPCam, mx - x, my - y
            ShowEntity VPMarkerDragEN
            Local groundEN = PickedEntity()
            If groundEN = VPGround
                Local newX# = PickedX#()
                Local newZ# = PickedZ#()
                ; Update marker position
                PositionEntity VPMarkerDragEN, newX#, EntityY#(VPMarkerDragEN), newZ#
                ; Update the underlying Area field via the existing zone
                ; setter dispatch (handles Strict-mode dim-write trap).
                Loom_CommitMarkerCoord(VPMarkerDragArH, VPMarkerDragKind$, VPMarkerDragIdx, newX#, newZ#)
                VPDirty = True
            EndIf
        EndIf
    Else
        If VPMarkerDragging = True
            ; Commit on release. The per-frame updates already wrote
            ; through to the Area; just fire a toast + mark dirty.
            If LoomComposer <> Null Then Composer::markDirtyForKind(LoomComposer, "zone")
            Toast_Show("Moved " + VPMarkerDragKind$ + " " + Str(VPMarkerDragIdx), "success")
            WriteLog(LoomLog, "ZoneViewport: drag commit " + VPMarkerDragKind$ + " " + Str(VPMarkerDragIdx))
            VPMarkerDragging = False
            VPMarkerDragEN   = 0
            VPMarkerDragKind$ = ""
            VPMarkerDragIdx  = -1
        EndIf
    EndIf

    If inside = True
        Local wheel = Loom_MouseWheel()
        If wheel <> 0
            VPDistance# = VPDistance# - Float(wheel) * (VPDistance# * 0.08)
            If VPDistance# < 20.0 Then VPDistance# = 20.0
            If VPDistance# > 5000.0 Then VPDistance# = 5000.0
            VPDirty = True
        EndIf
    EndIf

    ; ---- MMB pan camera ----------------------------------------------------
    ; Middle-mouse drag translates the orbit center in camera-aligned
    ; XZ. Forward/right vectors derived from the current VPYaw so panning
    ; feels natural relative to the visible camera orientation. Pan speed
    ; scales with VPDistance so farther zooms produce larger per-pixel
    ; pan steps (keeps the apparent on-screen drag rate consistent).
    If MouseDown(3) = True And inside = True
        If VPPanning = False
            VPPanning = True
            VPPanLastMX = mx
            VPPanLastMY = my
        Else
            Local pdx = mx - VPPanLastMX
            Local pdy = my - VPPanLastMY
            Local panSpeed# = VPDistance# / 200.0
            ; Camera-relative axes in world XZ (yaw=0 looks along -Z):
            ;   forward (away from camera) = (Sin(yaw), 0, -Cos(yaw))
            ;   right                       = (Cos(yaw), 0, Sin(yaw))
            Local fwdX# = Sin(VPYaw#)
            Local fwdZ# = -Cos(VPYaw#)
            Local rgtX# = Cos(VPYaw#)
            Local rgtZ# = Sin(VPYaw#)
            ; Drag right (positive pdx) should slide scene LEFT under
            ; the camera, so subtract pdx * right.
            VPSceneCenterX# = VPSceneCenterX# - Float(pdx) * rgtX# * panSpeed# + Float(pdy) * fwdX# * panSpeed#
            VPSceneCenterZ# = VPSceneCenterZ# - Float(pdx) * rgtZ# * panSpeed# + Float(pdy) * fwdZ# * panSpeed#
            VPPanLastMX = mx
            VPPanLastMY = my
        EndIf
    Else
        VPPanning = False
    EndIf

    ; ---- Position camera by orbit math -------------------------------------
    Local yawRad# = VPYaw# * 3.14159 / 180.0
    ; ---- Highlight transition: only scale on change -----------------------
    ; Per-frame iteration over every marker was expensive on big zones
    ; (3000+ entities -> 3000+ ScaleEntity calls per frame). Now only
    ; the OLD highlighted marker shrinks back and the NEW one grows;
    ; if the highlight didn't change, no scaling work at all.
    If LoomZoneHighlightKind$ <> VPPrevHighlightKind$ Or LoomZoneHighlightIdx <> VPPrevHighlightIdx
        Local hm.ZoneViewportMarker
        For hm = Each ZoneViewportMarker
            If hm\Kind = VPPrevHighlightKind$ And hm\IndexN = VPPrevHighlightIdx And VPPrevHighlightKind$ <> ""
                ScaleEntity hm\EN, hm\BaseScale, hm\BaseScale, hm\BaseScale
            Else If hm\Kind = LoomZoneHighlightKind$ And hm\IndexN = LoomZoneHighlightIdx And LoomZoneHighlightKind$ <> ""
                ScaleEntity hm\EN, hm\BaseScale * 1.6, hm\BaseScale * 1.6, hm\BaseScale * 1.6
            EndIf
        Next
        VPPrevHighlightKind$ = LoomZoneHighlightKind$
        VPPrevHighlightIdx   = LoomZoneHighlightIdx
        VPDirty = True
    EndIf

    ; ---- Re-render to texture only when something changed ------------------
    ; Camera orbit/pan/zoom + marker drags + highlight transitions all
    ; set VPDirty = True. Static frames skip RenderWorld + the camera
    ; re-position math; the cached texture from the last render gets
    ; CopyRect'd to the back buffer below.
    If VPDirty = True
        Local cx# = VPSceneCenterX# + Cos(VPPitch#) * Sin(VPYaw#) * VPDistance#
        Local cy# = VP_SCENE_Y_OFFSET# + VPSceneCenterY# + Sin(VPPitch#) * VPDistance#
        Local cz# = VPSceneCenterZ# - Cos(VPPitch#) * Cos(VPYaw#) * VPDistance#
        PositionEntity VPCam, cx#, cy#, cz#
        PointEntity VPCam, VPGround

        ShowEntity VPCam
        SetBuffer TextureBuffer(VPRT)
        RenderWorld
        SetBuffer BackBuffer()
        HideEntity VPCam

        VPDirty = False
    EndIf

    ; Blit cached texture to back buffer every frame (back buffer is
    ; cleared at the top of renderFrame so we always need to repaint).
    CopyRect 0, 0, VP_RT_SIZE, VP_RT_SIZE, x, y, TextureBuffer(VPRT), BackBuffer()
    LoomBorder x, y, VP_RT_SIZE, VP_RT_SIZE, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B

    ; Legend overlay with live counts. Capitalized labels mirror the
    ; composer section names so the user can mentally map widget colors
    ; to the editable sections below the viewport.
    LoomText x + 8, y + 8,  "portals "   + Str(VPCountPortals),   LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B
    LoomText x + 8, y + 24, "spawns "    + Str(VPCountSpawns),    LOOM_ARCANE_500_R, LOOM_ARCANE_500_G, LOOM_ARCANE_500_B
    LoomText x + 8, y + 40, "triggers "  + Str(VPCountTriggers),  LOOM_WARNING_R, LOOM_WARNING_G, LOOM_WARNING_B
    LoomText x + 8, y + 56, "waypoints " + Str(VPCountWaypoints), 200, 200, 210
    ; X/Y/Z axis legend (matches the colored lines at scene origin)
    LoomText x + VP_RT_SIZE - 60, y + 8,  "X", 220, 60, 60
    LoomText x + VP_RT_SIZE - 48, y + 8,  "Y", 60, 220, 60
    LoomText x + VP_RT_SIZE - 36, y + 8,  "Z", 60, 120, 220

    If inside = True
        LoomText x + 8, y + VP_RT_SIZE - 18, "LMB=orbit  MMB=pan  wheel=zoom", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B
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
