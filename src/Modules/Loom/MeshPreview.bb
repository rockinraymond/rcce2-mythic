; =============================================================================
; Loom/MeshPreview.bb -- 3D actor / item mesh preview widget
; =============================================================================
;
; First cut of the deferred "3D viewport" workstream. Renders a single mesh
; (typically an actor's MeshIDs[0] base body) into a render-to-texture
; target via a dedicated camera + light, then DrawImages the result into
; the composer at the requested rect.
;
; Architecture:
;   - PreviewCam       Camera entity that renders only the preview scene
;   - PreviewLight     Single DirectionalLight so the mesh is visible
;   - PreviewRT        TextureBuffer the camera renders into
;   - PreviewMesh      Current mesh entity (loaded via GetMesh)
;   - PreviewMeshID    Cached ID so we don't reload on every frame
;
; The preview camera is positioned FAR from the world origin (10000 units
; up) so it can't accidentally see any future world geometry the editor
; might create. The mesh sits next to the camera in that isolated space.
;
; Render flow per frame (called by composer):
;   1. position the mesh + camera; tick auto-spin
;   2. SetBuffer(TextureBuffer(rt))
;   3. CameraClsColor + RenderWorld
;   4. SetBuffer(BackBuffer())   <- critical: restore 2D drawing target
;   5. DrawImage rt at (x, y)
;
; Non-Strict (matches Settings.bb / ImageCache.bb / Recents.bb -- all the
; "free-function-with-module-globals" files).
;
; Caveats:
;   - This is a first cut. No textures applied (face/body textures would
;     need additional GetTexture + EntityTexture calls).
;   - Mesh animation isn't ticked.
;   - Failure mode is graceful: if mesh load fails, the placeholder draws
;     instead.


; ---- Constants --------------------------------------------------------------
Const LOOM_PREVIEW_SIZE       = 192     ; render-target dim (square)
Const LOOM_PREVIEW_CAM_X#     = 0.0
Const LOOM_PREVIEW_CAM_Y#     = 10000.0 ; far from any world geometry
Const LOOM_PREVIEW_CAM_Z#     = 0.0
Const LOOM_PREVIEW_CAM_RANGE# = 50.0    ; pulled back to fit a typical actor
Const LOOM_PREVIEW_AUTOSPIN_DEG_PER_SEC# = 36.0   ; one full rotation / 10s


; ---- Module state -----------------------------------------------------------
Global PreviewCam        = 0
Global PreviewLight      = 0
Global PreviewRT         = 0          ; render-target texture handle
Global PreviewMesh       = 0          ; current mesh entity
Global PreviewMeshID     = -1         ; cached ID so we know when to reload
Global PreviewInitOK     = False      ; True once the camera + RT exist
Global PreviewSpinTime#  = 0.0
Global PreviewLastTickMs = 0          ; for delta calc
Global PreviewMeshScale# = 1.0        ; auto-fit scale for current mesh


; =============================================================================
; Loom_InitMeshPreview -- one-time setup of camera + light + render target.
; Called once from Loom.bb after Graphics3D + entity data load.
; =============================================================================
Function Loom_InitMeshPreview()
    If PreviewInitOK = True Then Return

    ; Render-target texture. Flag 1 = color (RGB) + 256 = render-target
    ; in Blitz3D's CreateTexture flag bitfield. (1 + 256 = 257)
    PreviewRT = CreateTexture(LOOM_PREVIEW_SIZE, LOOM_PREVIEW_SIZE, 1 + 256)
    If PreviewRT = 0
        WriteLog(LoomLog, "MeshPreview: CreateTexture failed -- preview disabled")
        Return
    EndIf
    TextureBlend PreviewRT, 0

    ; Camera positioned far from world origin so it can't accidentally
    ; see future world geometry. Renders only entities near (0, 10000, 0).
    PreviewCam = CreateCamera()
    PositionEntity PreviewCam, LOOM_PREVIEW_CAM_X#, LOOM_PREVIEW_CAM_Y# + 5.0, LOOM_PREVIEW_CAM_Z# - LOOM_PREVIEW_CAM_RANGE#
    PointEntity   PreviewCam, 0   ; will be re-pointed at the mesh each frame
    CameraClsColor PreviewCam, 16, 16, 22   ; dark stone-950 background
    CameraRange    PreviewCam, 0.1, 1000.0
    HideEntity     PreviewCam     ; only show when actively rendering

    ; Single directional light so the mesh is visible.
    PreviewLight = CreateLight(1)   ; 1 = directional
    PositionEntity PreviewLight, LOOM_PREVIEW_CAM_X#, LOOM_PREVIEW_CAM_Y# + 20.0, LOOM_PREVIEW_CAM_Z# - 10.0
    RotateEntity   PreviewLight, 45, -30, 0
    LightColor     PreviewLight, 255, 255, 230

    PreviewInitOK = True
    PreviewLastTickMs = MilliSecs()
    WriteLog(LoomLog, "MeshPreview: initialized (RT=" + LOOM_PREVIEW_SIZE + "x" + LOOM_PREVIEW_SIZE + ")")
End Function


; =============================================================================
; Loom_FreePreviewMesh -- free the currently-loaded preview mesh (if any).
; Called when switching to a different mesh ID, and from Loom shutdown.
; =============================================================================
Function Loom_FreePreviewMesh()
    If PreviewMesh <> 0
        FreeEntity PreviewMesh
        PreviewMesh = 0
    EndIf
    PreviewMeshID = -1
End Function


; =============================================================================
; Loom_LoadPreviewMesh -- swap the preview mesh to a new ID. Reuses
; Media.bb's GetMesh which lazy-loads + caches. We pass Duplicate=True
; so we get our own owned entity to position freely. The CACHED entity
; (LoadedMeshes(ID)) stays hidden where Media.bb put it.
; =============================================================================
Function Loom_LoadPreviewMesh(meshID)
    If PreviewInitOK = False Then Return False
    If meshID = PreviewMeshID Then Return (PreviewMesh <> 0)
    If meshID <= 0
        Loom_FreePreviewMesh()
        Return False
    EndIf

    Loom_FreePreviewMesh()

    Local ent = GetMesh(meshID, True)
    If ent = 0
        PreviewMeshID = meshID    ; remember the miss so we don't retry every frame
        Return False
    EndIf

    ; Position the mesh next to the preview camera in the isolated 10000-y
    ; region. The GetMesh-applied scale is already set; we add auto-fit
    ; positioning relative to the camera.
    PositionEntity ent, LOOM_PREVIEW_CAM_X#, LOOM_PREVIEW_CAM_Y#, LOOM_PREVIEW_CAM_Z#
    ShowEntity ent

    PreviewMesh = ent
    PreviewMeshID = meshID
    WriteLog(LoomLog, "MeshPreview: loaded mesh ID " + meshID)
    Return True
End Function


; =============================================================================
; Loom_DrawMeshPreview -- public entry: ensure the mesh is loaded, tick
; auto-spin, render to RT, paint at (x, y) at (size x size). Falls back to
; the same "?" placeholder ImageCache uses if anything goes wrong.
; =============================================================================
Function Loom_DrawMeshPreview(meshID, x, y, size)
    If PreviewInitOK = False
        Loom_DrawMeshPlaceholder(x, y, size, "init failed")
        Return False
    EndIf

    Local loaded = Loom_LoadPreviewMesh(meshID)
    If loaded = False
        Loom_DrawMeshPlaceholder(x, y, size, "no mesh")
        Return False
    EndIf

    ; Tick auto-spin in degrees, based on real wall-clock delta so the
    ; spin rate stays consistent even if frame time varies.
    Local nowMs = MilliSecs()
    Local deltaMs = nowMs - PreviewLastTickMs
    If deltaMs < 0 Then deltaMs = 0
    If deltaMs > 1000 Then deltaMs = 1000   ; clamp giant deltas (alt-tab away/back)
    PreviewLastTickMs = nowMs
    PreviewSpinTime# = PreviewSpinTime# + (Float(deltaMs) / 1000.0) * LOOM_PREVIEW_AUTOSPIN_DEG_PER_SEC#
    If PreviewSpinTime# >= 360.0 Then PreviewSpinTime# = PreviewSpinTime# - 360.0

    ; Apply spin to the mesh's Y rotation. Camera stays fixed.
    RotateEntity PreviewMesh, 0, PreviewSpinTime#, 0

    ; Render the scene into the RT. SetBuffer must be restored to
    ; BackBuffer afterward or all 2D drawing breaks.
    ShowEntity PreviewCam
    SetBuffer TextureBuffer(PreviewRT)
    RenderWorld
    SetBuffer BackBuffer()
    HideEntity PreviewCam

    ; Draw the RT as an Image. CreateTexture with flag 256 produces a
    ; texture-buffer that doubles as an image source; DrawImage works
    ; via Blitz3D's TextureBuffer/CopyRect dance. Easiest: use
    ; CopyRect into a regular image. For first cut, just blit the
    ; texture directly via a temporary image.
    Loom_BlitPreviewTexture(PreviewRT, x, y, size)
    Return True
End Function


; =============================================================================
; Loom_BlitPreviewTexture -- copy the RT to the BackBuffer at the target
; rect. Blitz3D's DrawImage doesn't take a texture handle, only an Image,
; so we use CopyRect from the texture buffer.
; =============================================================================
Function Loom_BlitPreviewTexture(rt, x, y, size)
    ; CopyRect copies pixels between buffers. Source = the RT's buffer,
    ; dest = the BackBuffer at the requested position. Since RT is
    ; LOOM_PREVIEW_SIZE square and we want it at (x, y) of `size`
    ; pixels, we scale by copying the whole RT then stretching... but
    ; CopyRect doesn't stretch. For first cut, render at exactly
    ; LOOM_PREVIEW_SIZE and ignore the size parameter.
    CopyRect 0, 0, LOOM_PREVIEW_SIZE, LOOM_PREVIEW_SIZE, x, y, TextureBuffer(rt), BackBuffer()

    ; Brass border around the preview so it reads as a discrete widget.
    LoomBorder x, y, LOOM_PREVIEW_SIZE, LOOM_PREVIEW_SIZE, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B
End Function


; =============================================================================
; Loom_DrawMeshPlaceholder -- stone-900 box with "?" + status text for
; missing-mesh / init-failed cases. Same visual shape as ImageCache's
; placeholder so the composer layout stays stable.
; =============================================================================
Function Loom_DrawMeshPlaceholder(x, y, size, note$)
    Local s = LOOM_PREVIEW_SIZE
    If size > 0 Then s = size
    LoomFill x, y, s, s, LOOM_STONE_900_R, LOOM_STONE_900_G, LOOM_STONE_900_B
    LoomBorder x, y, s, s, LOOM_STONE_700_R, LOOM_STONE_700_G, LOOM_STONE_700_B
    LoomText x + (s / 2) - 4, y + (s / 2) - 16, "?", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B
    LoomText x + 8, y + s - 18, note, LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B
End Function


; =============================================================================
; Loom_ShutdownMeshPreview -- free GPU resources on Loom exit. Called from
; Loom.bb after the main loop ends.
; =============================================================================
Function Loom_ShutdownMeshPreview()
    Loom_FreePreviewMesh()
    If PreviewCam <> 0 Then FreeEntity PreviewCam : PreviewCam = 0
    If PreviewLight <> 0 Then FreeEntity PreviewLight : PreviewLight = 0
    If PreviewRT <> 0 Then FreeTexture PreviewRT : PreviewRT = 0
    PreviewInitOK = False
End Function
