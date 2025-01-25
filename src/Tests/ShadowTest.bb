; -------------------------------------------------------------------------------------------------------------------
; Shadow System Test Scene
; -------------------------------------------------------------------------------------------------------------------

; Include shadow system
Include "Modules/Libraries/NewShadowSystem.bb"

; Graphics settings
Graphics3D 800, 600, 32, 2
SetBuffer BackBuffer()

; Enable debug mode for visualization
DEBUG_MODE = True
DEBUG_LOG_ENABLED = True

; Initialize shadow system
InitShadowSystem()

; Create camera
Global camera = CreateCamera()
PositionEntity camera, 0, 20, -30
RotateEntity camera, 20, 0, 0
CameraRange camera, 0.1, 1000
NameEntity camera, "MainCamera"

; Create ambient light
AmbientLight 128, 128, 128

; Create light
Global light = CreateLight(1) ; Type 1 = Point light
PositionEntity light, 10, 15, -10 ; Position closer to scene
LightRange light, 50 ; Smaller range for more concentrated shadows
LightColor light, 255, 255, 255
NameEntity light, "MainLight"

; Create shadow light
Global shadowLight.ShadowLight = CreateShadowLight(light, 50, 1.0)

; Create floor (receiver) - using a flat cube
Global floor = CreateCube()
EntityColor floor, 200, 200, 200
MoveEntity floor, 0, -5, 0
ScaleEntity floor, 20, 0.1, 20 ; Smaller floor for more visible shadows
NameEntity floor, "Floor"

; Create shadow receiver for floor
Global floorReceiver.ShadowReceiver = CreateShadowReceiver(floor)

; Create cube (caster)
Global cube = CreateCube()
PositionEntity cube, 0, 0, 0
EntityColor cube, 255, 0, 0
ScaleEntity cube, 2, 2, 2
NameEntity cube, "Cube"

; Create shadow caster for cube
Global cubeCaster.ShadowCaster = CreateShadowCaster(cube, 1024) ; Higher resolution shadow map

; Main loop
While Not KeyDown(1)
    ; Rotate cube slowly
    TurnEntity cube, 0.1, 0.2, 0.0
    
    ; Update shadows
    UpdateShadows(camera)
    
    ; Move light with keys
    If KeyDown(203) Then MoveEntity light, -0.1, 0, 0 ; Left
    If KeyDown(205) Then MoveEntity light, 0.1, 0, 0  ; Right
    If KeyDown(200) Then MoveEntity light, 0, 0.1, 0  ; Up
    If KeyDown(208) Then MoveEntity light, 0, -0.1, 0 ; Down
    
    UpdateWorld
    RenderWorld
    
    ; Display debug info
    Text 10, 10, "Arrow keys - Move light"
    Text 10, 30, "ESC - Exit"
    Text 10, 50, "Light pos: " + EntityX(light) + ", " + EntityY(light) + ", " + EntityZ(light)
    Text 10, 70, "Cube pos: " + EntityX(cube) + ", " + EntityY(cube) + ", " + EntityZ(cube)
    Text 10, 90, "In range: " + IsInRange(cube, light, 50)
    Text 10, 110, "Shadow mesh exists: " + (floorReceiver\shadowMesh <> 0)
    Text 10, 130, "Shadow vertices: " + CountVertices(GetSurface(floorReceiver\shadowMesh, 1))
    
    Flip
Wend

End 