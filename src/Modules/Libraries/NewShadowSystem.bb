; -------------------------------------------------------------------------------------------------------------------
; Modern Shadow System - A clean, debuggable shadow implementation
; -------------------------------------------------------------------------------------------------------------------

; Core Types
Type ShadowCaster
    Field entity        ; The entity casting the shadow
    Field shadowMap     ; The texture used for the shadow map
    Field resolution    ; Resolution of the shadow map
    Field isTranslucent ; Whether the caster is translucent
    Field boundingSphere# ; Radius of bounding sphere for culling
    Field debugMode     ; Enable extra debug visualization
End Type

Type ShadowReceiver
    Field entity       ; The entity receiving shadows
    Field boundingSphere# ; Radius of bounding sphere for culling
    Field debugMode    ; Enable extra debug visualization
    Field shadowMesh   ; The mesh used for shadows
    Field scaleX#     ; Entity's X scale
    Field scaleY#     ; Entity's Y scale
    Field scaleZ#     ; Entity's Z scale
    Field surfaceNormalMap ; Normal map texture for non-flat surfaces
End Type

Type ShadowLight 
    Field entity      ; The light entity
    Field range#      ; Light's effective range
    Field intensity#  ; Light's intensity (0-1)
    Field debugMode   ; Enable extra debug visualization
    Field softness#   ; Shadow edge softness (0-1)
End Type

; Debug visualization colors
Global DEBUG_COLOR_CASTER_BOUNDS = $FF0000    ; Red
Global DEBUG_COLOR_RECEIVER_BOUNDS = $00FF00   ; Green  
Global DEBUG_COLOR_LIGHT_RANGE = $0000FF      ; Blue
Global DEBUG_COLOR_SHADOW_VOLUME = $FFFF00    ; Yellow

; Shadow quality settings
Global SHADOW_MAP_SIZE = 512        ; Default shadow map resolution
Global SHADOW_BIAS# = 0.001         ; Depth bias to prevent shadow acne
Global SHADOW_SOFTNESS# = 1.0       ; Shadow edge softness
Global SHADOW_AMBIENT_LIGHT# = 0.2  ; Minimum shadow darkness
Global SHADOW_MAX_STEPS = 8         ; Maximum ray march steps for surface intersection
Global SHADOW_STEP_SIZE# = 0.1      ; Step size for ray marching

; Performance settings
Global MAX_SHADOW_CASTERS = 32
Global MAX_SHADOW_RECEIVERS = 64  
Global MAX_SHADOW_LIGHTS = 8
Global CULLING_ENABLED = True
Global MAX_VERTICES = 65536
Dim VertexMap(MAX_VERTICES) ; Global array for vertex mapping

; Debug settings
Global DEBUG_MODE = False           ; Master debug switch
Global DEBUG_SHOW_BOUNDS = False    ; Show bounding volumes
Global DEBUG_LOG_ENABLED = False    ; Enable detailed logging

; -------------------------------------------------------------------------------------------------------------------
; Core shadow system functions
; -------------------------------------------------------------------------------------------------------------------

Function InitShadowSystem()
    If DEBUG_LOG_ENABLED Then DebugLog "Initializing shadow system..."
    
    ; Create debug visualization meshes if needed
    If DEBUG_MODE
        CreateDebugVisualizers()
    EndIf
    
    Return True
End Function

Function CreateShadowCaster.ShadowCaster(entity, resolution=0, isTranslucent=False)
    If resolution = 0
        resolution = SHADOW_MAP_SIZE
    EndIf
    
    Local caster.ShadowCaster = New ShadowCaster()
    
    caster\entity = entity
    caster\resolution = resolution
    caster\isTranslucent = isTranslucent
    
    ; Create shadow map texture
    caster\shadowMap = CreateTexture(resolution, resolution, 1+2+16+32)
    TextureBlend caster\shadowMap, 2 ; Multiplicative blending
    
    ; Calculate bounding sphere
    caster\boundingSphere# = CalculateBoundingSphere(entity)
    
    If DEBUG_LOG_ENABLED 
        DebugLog "Created shadow caster: " + EntityName$(entity)
    EndIf
    
    Return caster
End Function

Function CreateShadowReceiver.ShadowReceiver(entity)
    Local receiver.ShadowReceiver = New ShadowReceiver()
    
    receiver\entity = entity
    receiver\boundingSphere# = CalculateBoundingSphere(entity)
    
    ; Store current scale
    TFormVector 1, 1, 1, entity, 0
    receiver\scaleX# = TFormedX#()
    receiver\scaleY# = TFormedY#()
    receiver\scaleZ# = TFormedZ#()
    
    If DEBUG_LOG_ENABLED
        DebugLog "Created shadow receiver: " + EntityName$(entity)
        DebugLog "Receiver scale: " + receiver\scaleX# + ", " + receiver\scaleY# + ", " + receiver\scaleZ#
    EndIf
    
    Return receiver
End Function

Function CreateShadowLight.ShadowLight(entity, range#=100.0, intensity#=1.0)
    Local light.ShadowLight = New ShadowLight()
    
    light\entity = entity
    light\range# = range#
    light\intensity# = intensity#
    
    If DEBUG_LOG_ENABLED
        DebugLog "Created shadow light: " + EntityName$(entity)
    EndIf
    
    Return light
End Function

; -------------------------------------------------------------------------------------------------------------------
; Shadow update and rendering
; -------------------------------------------------------------------------------------------------------------------

Function UpdateShadows(camera)
    If Not camera Return
    
    If DEBUG_LOG_ENABLED Then DebugLog "Beginning shadow update..."
    
    ; Store current camera state
    CameraProjMode(camera, 0)
    
    ; Create shadow camera
    Local shadowCam = CreateCamera()
    CameraRange shadowCam, 0.1, 1000
    CameraClsMode shadowCam, True, True
    
    ; Update each light's shadows
    For l.ShadowLight = Each ShadowLight
        UpdateLightShadows(l, shadowCam)
    Next
    
    ; Restore camera state
    CameraProjMode camera, 1
    
    ; Cleanup
    FreeEntity shadowCam
    
    If DEBUG_LOG_ENABLED Then DebugLog "Shadow update complete"
End Function

Function UpdateLightShadows(light.ShadowLight, shadowCam)
    If light = Null Or shadowCam = 0 Return
    
    Local lightX# = EntityX#(light\entity, True)
    Local lightY# = EntityY#(light\entity, True)
    Local lightZ# = EntityZ#(light\entity, True)
    
    ; Update each caster for this light
    For caster.ShadowCaster = Each ShadowCaster
        ; Skip if outside light range
        If IsInRange(caster\entity, light\entity, light\range#)
            
            ; Render shadow map
            RenderShadowMap(caster, light, shadowCam)
            
            ; Apply shadows to receivers
            For receiver.ShadowReceiver = Each ShadowReceiver
                If IsInRange(receiver\entity, light\entity, light\range#)
                    If DEBUG_LOG_ENABLED
                        DebugLog "Projecting shadow from " + EntityName$(caster\entity) + " to " + EntityName$(receiver\entity)
                        DebugLog "Light position: " + lightX# + ", " + lightY# + ", " + lightZ#
                    EndIf
                    
                    ApplyShadowToReceiver(caster, light, receiver)
                EndIf
            Next
        EndIf
    Next
    
    If DEBUG_MODE
        UpdateDebugVisualizers(light)
    EndIf
End Function

; -------------------------------------------------------------------------------------------------------------------
; Helper functions
; -------------------------------------------------------------------------------------------------------------------

Function CalculateBoundingSphere#(entity)
    ; Calculate radius that encompasses all vertices
    Local maxRadius# = 0
    
    For s = 1 To CountSurfaces(entity)
        Local surf = GetSurface(entity, s)
        For v = 0 To CountVertices(surf)-1
            Local vx# = VertexX#(surf, v)
            Local vy# = VertexY#(surf, v)
            Local vz# = VertexZ#(surf, v)
            Local radius# = Sqr(vx#*vx# + vy#*vy# + vz#*vz#)
            If radius# > maxRadius# Then maxRadius# = radius#
        Next
    Next
    
    Return maxRadius#
End Function

Function IsInRange(entity1, entity2, range#)
    Local dx# = EntityX#(entity1, True) - EntityX#(entity2, True)
    Local dy# = EntityY#(entity1, True) - EntityY#(entity2, True)
    Local dz# = EntityZ#(entity1, True) - EntityZ#(entity2, True)
    Local dist# = Sqr(dx#*dx# + dy#*dy# + dz#*dz#)
    Return dist# <= range#
End Function

; -------------------------------------------------------------------------------------------------------------------
; Debug functions
; -------------------------------------------------------------------------------------------------------------------

Function SetDebugMode(enabled)
    DEBUG_MODE = enabled
    If enabled
        CreateDebugVisualizers()
    Else
        CleanupDebugVisualizers()
    EndIf
End Function

; -------------------------------------------------------------------------------------------------------------------
; Shadow rendering functions
; -------------------------------------------------------------------------------------------------------------------

Function RenderShadowMap(caster.ShadowCaster, light.ShadowLight, shadowCam)
    If caster = Null Or light = Null Or shadowCam = 0 Return
    
    If DEBUG_LOG_ENABLED 
        DebugLog "Rendering shadow map for caster: " + EntityName$(caster\entity)
    EndIf
    
    ; Position and orient shadow camera
    PositionShadowCamera(shadowCam, light, caster)
    
    ; Set up render target
    SetBuffer TextureBuffer(caster\shadowMap)
    ClsColor 255, 255, 255
    Cls
    
    ; Configure shadow camera
    CameraViewport shadowCam, 0, 0, caster\resolution, caster\resolution
    
    ; Hide all entities except the shadow caster
    HideAllExcept(caster\entity)
    
    ; Render the shadow map
    RenderWorld
    
    ; Restore entity states
    ShowAll()
    
    ; Reset buffer
    SetBuffer BackBuffer()
    ClsColor 0,0,0
End Function

Function ApplyShadowToReceiver(caster.ShadowCaster, light.ShadowLight, receiver.ShadowReceiver)
    If caster = Null Or light = Null Or receiver = Null Return
    
    If DEBUG_LOG_ENABLED
        DebugLog "Applying shadow from " + EntityName$(caster\entity) + " to " + EntityName$(receiver\entity)
    EndIf
    
    ; Create or get shadow mesh for this receiver
    Local shadowMesh = GetShadowMesh(receiver)
    If Not shadowMesh Return
    
    ; Project shadow onto receiver
    ProjectShadow(shadowMesh, caster\entity, light, receiver, caster\shadowMap)
    
    If DEBUG_MODE
        VisualizeShadowProjection(shadowMesh, light, caster, receiver)
    EndIf
End Function

; -------------------------------------------------------------------------------------------------------------------
; Shadow projection helpers
; -------------------------------------------------------------------------------------------------------------------

Function PositionShadowCamera(shadowCam, light.ShadowLight, caster.ShadowCaster)
    ; Get light position
    Local lightX# = EntityX#(light\entity, True)
    Local lightY# = EntityY#(light\entity, True)
    Local lightZ# = EntityZ#(light\entity, True)
    
    ; Get caster position
    Local casterX# = EntityX#(caster\entity, True)
    Local casterY# = EntityY#(caster\entity, True)
    Local casterZ# = EntityZ#(caster\entity, True)
    
    ; Position camera at light looking at caster
    PositionEntity shadowCam, lightX#, lightY#, lightZ#
    PointEntity shadowCam, caster\entity
    
    ; Adjust camera properties for shadow mapping
    CameraZoom shadowCam, 8 ; Less zoom for wider view
    CameraRange shadowCam, 1, light\range#
    CameraClsMode shadowCam, True, True
End Function

Function GetShadowMesh(receiver.ShadowReceiver)
    ; If we already have a shadow mesh, free it
    If receiver\shadowMesh
        FreeEntity receiver\shadowMesh
        receiver\shadowMesh = 0
    EndIf
    
    ; Create new shadow mesh in world space (no parent)
    Local mesh = CreateMesh()
    
    ; Set mesh properties
    EntityBlend mesh, 2 ; Multiply blend mode
    EntityAlpha mesh, 0.5 ; Semi-transparent
    EntityFX mesh, 8 ; Fullbright + enable depth writes
    CreateSurface(mesh) ; Create initial surface
    
    ; Store mesh in receiver
    receiver\shadowMesh = mesh
    
    Return mesh
End Function

Function CalculateShadowMatrix#(light.ShadowLight, caster.ShadowCaster)
    ; Calculate light direction vector
    Local lx# = EntityX#(light\entity, True) - EntityX#(caster\entity, True)
    Local ly# = EntityY#(light\entity, True) - EntityY#(caster\entity, True)
    Local lz# = EntityZ#(light\entity, True) - EntityZ#(caster\entity, True)
    
    ; Normalize
    Local len# = Sqr(lx#*lx# + ly#*ly# + lz#*lz#)
    lx# = lx# / len#
    ly# = ly# / len#
    lz# = lz# / len#
    
    ; Create projection matrix (simplified for now)
    Return 1.0
End Function

Function ProjectShadow(shadowMesh, casterEntity, light.ShadowLight, receiver.ShadowReceiver, shadowMap)
    Local surfCount = CountSurfaces(casterEntity)
    If surfCount = 0 Return
    
    If DEBUG_LOG_ENABLED
        DebugLog "Projecting shadow from " + EntityName$(casterEntity)
    EndIf
    
    ; Get light position in world space
    Local lightX# = EntityX#(light\entity, True)
    Local lightY# = EntityY#(light\entity, True)
    Local lightZ# = EntityZ#(light\entity, True)
    
    ; Get the shadow surface
    Local shadowSurf = GetSurface(shadowMesh, 1)
    ClearSurface shadowSurf
    
    ; Get floor Y position (receiver)
    Local floorY# = EntityY#(receiver\entity, True)
    
    ; Process each surface of the caster
    For s = 1 To surfCount
        Local casterSurf = GetSurface(casterEntity, s)
        Local vertCount = CountVertices(casterSurf)
        
        ; Project vertices and create shadow geometry
        For v = 0 To vertCount-1
            ; Get vertex position in object space
            Local vx# = VertexX#(casterSurf, v)
            Local vy# = VertexY#(casterSurf, v)
            Local vz# = VertexZ#(casterSurf, v)
            
            ; Transform to world space
            TFormPoint vx#, vy#, vz#, casterEntity, 0
            vx# = TFormedX#()
            vy# = TFormedY#()
            vz# = TFormedZ#()
            
            ; Calculate direction from light to vertex
            Local dx# = vx# - lightX#
            Local dy# = vy# - lightY#
            Local dz# = vz# - lightZ#
            
            ; Simple projection to floor plane
            If Abs(dy#) > 0.0001 ; Avoid division by zero
                Local t# = (floorY# - lightY#) / dy#
                Local px# = lightX# + dx# * t#
                Local pz# = lightZ# + dz# * t#
                
                ; Add vertex to shadow mesh with a larger offset from the floor
                VertexMap(v) = AddVertex(shadowSurf, px#, floorY# + 0.5, pz#)
                
                ; Set shadow color (darker and more opaque)
                VertexColor shadowSurf, VertexMap(v), 0, 0, 0
            Else
                VertexMap(v) = -1 ; Mark invalid projection
            EndIf
        Next
        
        ; Copy triangles from caster using mapped vertices
        For t = 0 To CountTriangles(casterSurf)-1
            Local v0 = TriangleVertex(casterSurf, t, 0)
            Local v1 = TriangleVertex(casterSurf, t, 1)
            Local v2 = TriangleVertex(casterSurf, t, 2)
            
            ; Only add triangle if all vertices were projected successfully
            If VertexMap(v0) >= 0 And VertexMap(v1) >= 0 And VertexMap(v2) >= 0
                AddTriangle shadowSurf, VertexMap(v0), VertexMap(v1), VertexMap(v2)
            EndIf
        Next
    Next
End Function

Function RayMarchIntersect#(startX#, startY#, startZ#, dirX#, dirY#, dirZ#, target, hitX#, hitY#, hitZ#)
    Local currentX# = startX#
    Local currentY# = startY#
    Local currentZ# = startZ#
    
    ; Get target mesh surface
    Local surf = GetSurface(target, 1)
    If surf = 0 Return False
    
    For s = 1 To SHADOW_MAX_STEPS
        ; Move along ray
        currentX# = currentX# + dirX# * SHADOW_STEP_SIZE#
        currentY# = currentY# + dirY# * SHADOW_STEP_SIZE#
        currentZ# = currentZ# + dirZ# * SHADOW_STEP_SIZE#
        
        ; Transform point to target's local space
        TFormPoint currentX#, currentY#, currentZ#, 0, target
        Local localX# = TFormedX#()
        Local localY# = TFormedY#()
        Local localZ# = TFormedZ#()
        
        ; Check intersection with each triangle
        For t = 0 To CountTriangles(surf)-1
            Local v0 = TriangleVertex(surf, t, 0)
            Local v1 = TriangleVertex(surf, t, 1)
            Local v2 = TriangleVertex(surf, t, 2)
            
            ; Get triangle vertices
            Local x0# = VertexX#(surf, v0)
            Local y0# = VertexY#(surf, v0)
            Local z0# = VertexZ#(surf, v0)
            
            Local x1# = VertexX#(surf, v1)
            Local y1# = VertexY#(surf, v1)
            Local z1# = VertexZ#(surf, v1)
            
            Local x2# = VertexX#(surf, v2)
            Local y2# = VertexY#(surf, v2)
            Local z2# = VertexZ#(surf, v2)
            
            ; Check if point is inside triangle's bounding box first (optimization)
            Local minX# = Min(Min(x0#, x1#), x2#) - SHADOW_BIAS#
            Local maxX# = Max(Max(x0#, x1#), x2#) + SHADOW_BIAS#
            Local minY# = Min(Min(y0#, y1#), y2#) - SHADOW_BIAS#
            Local maxY# = Max(Max(y0#, y1#), y2#) + SHADOW_BIAS#
            Local minZ# = Min(Min(z0#, z1#), z2#) - SHADOW_BIAS#
            Local maxZ# = Max(Max(z0#, z1#), z2#) + SHADOW_BIAS#
            
            If localX# >= minX# And localX# <= maxX# And localY# >= minY# And localY# <= maxY# And localZ# >= minZ# And localZ# <= maxZ#
                ; Point is in bounding box, do detailed triangle test
                If PointInTriangle(localX#, localY#, localZ#, x0#, y0#, z0#, x1#, y1#, z1#, x2#, y2#, z2#)
                    ; Transform hit point back to world space
                    TFormPoint currentX#, currentY#, currentZ#, target, 0
                    hitX# = TFormedX#()
                    hitY# = TFormedY#()
                    hitZ# = TFormedZ#()
                    Return True
                EndIf
            EndIf
        Next
    Next
    
    Return False
End Function

Function PointInTriangle#(px#, py#, pz#, x0#, y0#, z0#, x1#, y1#, z1#, x2#, y2#, z2#)
    ; Calculate triangle normal
    Local nx# = (y1# - y0#) * (z2# - z0#) - (z1# - z0#) * (y2# - y0#)
    Local ny# = (z1# - z0#) * (x2# - x0#) - (x1# - x0#) * (z2# - z0#)
    Local nz# = (x1# - x0#) * (y2# - y0#) - (y1# - y0#) * (x2# - x0#)
    
    ; Normalize normal
    Local len# = Sqr(nx#*nx# + ny#*ny# + nz#*nz#)
    If len# = 0 Return False
    nx# = nx# / len#
    ny# = ny# / len#
    nz# = nz# / len#
    
    ; Project point and triangle onto dominant axis plane
    Local absX# = Abs(nx#)
    Local absY# = Abs(ny#)
    Local absZ# = Abs(nz#)
    
    Local u0#, v0#, u1#, v1#, u2#, v2#, pu#, pv#
    
    ; Choose largest normal component to determine projection plane
    If absX# >= absY# And absX# >= absZ#
        ; Project onto YZ plane
        u0# = y0# : v0# = z0#
        u1# = y1# : v1# = z1#
        u2# = y2# : v2# = z2#
        pu# = py# : pv# = pz#
    ElseIf absY# >= absX# And absY# >= absZ#
        ; Project onto XZ plane
        u0# = x0# : v0# = z0#
        u1# = x1# : v1# = z1#
        u2# = x2# : v2# = z2#
        pu# = px# : pv# = pz#
    Else
        ; Project onto XY plane
        u0# = x0# : v0# = y0#
        u1# = x1# : v1# = y1#
        u2# = x2# : v2# = y2#
        pu# = px# : pv# = py#
    EndIf
    
    ; Calculate barycentric coordinates
    Local area# = (u1# - u0#) * (v2# - v0#) - (u2# - u0#) * (v1# - v0#)
    If area# = 0 Return False
    
    Local s# = ((u1# - u0#) * (pv# - v0#) - (pu# - u0#) * (v1# - v0#)) / area#
    Local t# = ((pu# - u0#) * (v2# - v0#) - (u2# - u0#) * (pv# - v0#)) / area#
    
    ; Point is inside triangle if barycentric coordinates are all positive and sum to <= 1
    Return s# >= 0 And t# >= 0 And (s# + t#) <= 1
End Function

Function GetSurfaceNormalAtPoint(entity, x#, y#, z#, nx#, ny#, nz#)
    Local surf = GetSurface(entity, 1)
    If surf = 0 
        nx# = 0
        ny# = 1
        nz# = 0
        Return
    EndIf
    
    ; Find closest triangle and interpolate normal
    Local closestDist# = 1000000
    Local closestTri = -1
    
    For t = 0 To CountTriangles(surf)-1
        Local v0 = TriangleVertex(surf, t, 0)
        Local v1 = TriangleVertex(surf, t, 1)
        Local v2 = TriangleVertex(surf, t, 2)
        
        ; Get triangle center
        Local cx# = (VertexX#(surf,v0) + VertexX#(surf,v1) + VertexX#(surf,v2)) / 3
        Local cy# = (VertexY#(surf,v0) + VertexY#(surf,v1) + VertexY#(surf,v2)) / 3
        Local cz# = (VertexZ#(surf,v0) + VertexZ#(surf,v1) + VertexZ#(surf,v2)) / 3
        
        ; Calculate distance to point
        Local dx# = x# - cx#
        Local dy# = y# - cy#
        Local dz# = z# - cz#
        Local dist# = Sqr(dx#*dx# + dy#*dy# + dz#*dz#)
        
        If dist# < closestDist#
            closestDist# = dist#
            closestTri = t
        EndIf
    Next
    
    If closestTri >= 0
        ; Get vertex normals
        Local vv0 = TriangleVertex(surf, closestTri, 0)
        Local vv1 = TriangleVertex(surf, closestTri, 1)
        Local vv2 = TriangleVertex(surf, closestTri, 2)
        
        nx# = (VertexNX#(surf,vv0) + VertexNX#(surf,vv1) + VertexNX#(surf,vv2)) / 3
        ny# = (VertexNY#(surf,vv0) + VertexNY#(surf,vv1) + VertexNY#(surf,vv2)) / 3
        nz# = (VertexNZ#(surf,vv0) + VertexNZ#(surf,vv1) + VertexNZ#(surf,vv2)) / 3
        
        ; Normalize
        Local len# = Sqr(nx#*nx# + ny#*ny# + nz#*nz#)
        If len# > 0
            nx# = nx# / len#
            ny# = ny# / len#
            nz# = nz# / len#
        Else
            nx# = 0
            ny# = 1
            nz# = 0
        EndIf
    Else
        nx# = 0
        ny# = 1
        nz# = 0
    EndIf
End Function

; -------------------------------------------------------------------------------------------------------------------
; Entity visibility helpers
; -------------------------------------------------------------------------------------------------------------------

Function HideAllExcept(exceptEntity)
    For entity.ShadowCaster = Each ShadowCaster
        If entity\entity <> exceptEntity
            HideEntity entity\entity
        EndIf
    Next
End Function

Function ShowAll()
    For entity.ShadowCaster = Each ShadowCaster
        ShowEntity entity\entity
    Next
End Function

; -------------------------------------------------------------------------------------------------------------------
; Debug visualization
; -------------------------------------------------------------------------------------------------------------------
Global DEBUG_SPHERE

Function CreateDebugVisualizers()
    ; Create debug sphere for showing ranges
    DEBUG_SPHERE = CreateSphere()
    EntityColor DEBUG_SPHERE, 255, 0, 0
    EntityAlpha DEBUG_SPHERE, 0.3
    EntityFX DEBUG_SPHERE, 1+16 ; Full bright + no backface culling
    HideEntity DEBUG_SPHERE
End Function

Function CleanupDebugVisualizers()
    If DEBUG_SPHERE
        FreeEntity DEBUG_SPHERE
        DEBUG_SPHERE = 0
    EndIf
End Function

Function UpdateDebugVisualizers(light.ShadowLight)
    If DEBUG_SPHERE = 0 Return
    
    ; Show light range
    ShowEntity DEBUG_SPHERE
    PositionEntity DEBUG_SPHERE, EntityX#(light\entity, True), EntityY#(light\entity, True), EntityZ#(light\entity, True)
    ScaleEntity DEBUG_SPHERE, light\range#, light\range#, light\range#
    EntityColor DEBUG_SPHERE, 0, 0, 255
End Function

Function VisualizeShadowProjection(shadowMesh, light.ShadowLight, caster.ShadowCaster, receiver.ShadowReceiver)
    If DEBUG_MODE = False Return
    
    ; Visualize shadow volume (simplified)
    WireFrame True
    EntityColor shadowMesh, 255, 255, 0
    EntityAlpha shadowMesh, 0.5
    WireFrame False
End Function 

Function Min(a#, b#)
    If a# < b# Then Return a# Else Return b#
End Function

Function Max(a#, b#)
    If a# > b# Then Return a# Else Return b#
End Function
