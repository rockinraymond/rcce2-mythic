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
CameraClsColor camera, 0, 0, 128
PositionEntity camera, 0, 10, -20
RotateEntity camera, 20, 0, 0
CameraRange camera, 0.1, 1000
NameEntity camera, "MainCamera"

; Create ambient light
AmbientLight 128, 128, 128

; Create light
Global light = CreateLight()
NameEntity light, "MainLight"
PositionEntity light, 10, 15, -10
LightRange light, 50 ; Smaller range for more concentrated shadows
LightColor light, 255, 255, 255

; Create shadow light
Global shadowLight.ShadowLight = CreateShadowLight(light, 50, 1.0)

; Create floor (receiver) - using a flat cube
Global floor = CreateCube()
EntityColor floor, 128, 128, 128
MoveEntity floor, 0, -5, 0
ScaleEntity floor, 10, 0.1, 10 ; Smaller floor for more visible shadows
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
Local angle# = 0
While Not KeyDown(1)
    ; Update camera position
    angle# = angle# + 0.5
    PositionEntity camera, Sin(angle#) * 20, 10, Cos(angle#) * 20
    PointEntity camera, cube
    
    ; Handle light movement
    If KeyDown(200) Then MoveEntity light, 0, 0, 0.1 ; Forward
    If KeyDown(208) Then MoveEntity light, 0, 0, -0.1 ; Back
    If KeyDown(203) Then MoveEntity light, -0.1, 0, 0 ; Left
    If KeyDown(205) Then MoveEntity light, 0.1, 0, 0 ; Right

    ; Update shadows
    UpdateShadows(camera)
    
    RenderWorld
    
    ; Update debug info
    Text 5, 25, "Arrow keys - Move light"
    Text 5, 45, "ESC - Exit"
    Text 5, 65, "Light pos: " + EntityX#(light) + ", " + EntityY#(light) + ", " + EntityZ#(light)
    Text 5, 85, "Cube pos: " + EntityX#(cube) + ", " + EntityY#(cube) + ", " + EntityZ#(cube)
    Text 5, 105, "In range: " + IsInRange(cube, light, shadowLight\range)
    Text 5, 125, "Shadow mesh exists: " + (floorReceiver\shadowMesh <> 0)
    Text 5, 145, "Shadow vertices: " + CountVertices(GetSurface(floorReceiver\shadowMesh, 1))
    
    Flip
Wend

End 