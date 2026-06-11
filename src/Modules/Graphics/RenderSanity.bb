Strict

; SafeWriteOpen$ / SafeWriteCommit% (atomic-write helpers) for the first-
; failure diagnostic artifact in RenderSanityWriteDiagnostic below. Include is
; include-once, so the targets that already pull Logging.bb (Server, Client,
; GUE, Loom) dedupe it; Project Manager -- which reaches this file via
; RCCEGraphics and previously shipped without logging -- gains it here.
; Logging.bb is non-Strict and resolves its MainLog reference to an implicit 0
; when no Global MainLog exists (GUE already relies on exactly this today), so
; no target needs to declare MainLog for this to compile.
Include "Modules\Logging.bb"

; =============================================================================
; RenderSanity.bb -- boot-time detection + bounded recovery for issue #40
; =============================================================================
;
; Issue #40: randomly, on some launches, the application comes up with every
; DirectDraw surface dead -- no textures, no text, no loading screens, no
; mouse cursor -- under the Blitz-DDraw -> dgVoodoo -> D3D11 -> ReShade
; wrapper stack. It is unreproducible on demand, so the engine previously
; booted blind into the unusable state and the user's only remedy was to
; notice visually and restart by hand.
;
; This module turns that into: PROBE (do 2D surface blits AND a textured 3D
; render actually reach the backbuffer?), bounded RE-INIT (EndGraphics +
; Graphics3D again, the same remedy a manual restart performs, up to
; RS_MAX_REINITS times), and REPORT (log-friendly return codes for the caller,
; plus an AppTitle notice on final failure -- the title bar is OS-rendered,
; NOT a DirectDraw surface, so it stays visible precisely when everything in
; the window is dead).
;
; The probe is TWO checks (both must pass to call the surface healthy):
;   1. 2D blit + text reach the backbuffer (the original check).
;   2. A textured 3D quad samples its texture correctly. The #40 symptom is
;      specifically "2D draws fine but the textured world is blank/untextured"
;      -- a 2D-only probe FALSE-PASSES that exact failure, so the textured
;      check is what actually catches issue #40. See RenderSanityTextureProbe.
;
; On the FIRST detected probe failure of a session, EnsureRenderSanity writes a
; one-shot diagnostic artifact (Data\Logs\gfxprobe_diag.txt, atomic via
; SafeWriteOpen/SafeWriteCommit) BEFORE the curative re-init perturbs device
; state, capturing what's cheaply available for diagnosing the wrapper race.
;
; Depends on Logging.bb (atomic-write helpers) -- see the Include note at the
; top. Otherwise builtins only, so every target (Client, GUE, Loom, Project
; Manager via RCCEGraphics) can include it. Call EnsureRenderSanity immediately
; after Graphics3D and BEFORE InitExt / FastExt setup, so a re-init never has
; to tear down cached device state.
;
; Env hooks (for validation + field measurement, since the failure cannot
; be triggered on demand and agent-side runtime launch is unavailable):
;   RCCE_GFXPROBE=fail      every probe reports failure -- exercises the
;                           retry loop + AppTitle notice on a healthy box
;   RCCE_GFXPROBE=exit      write PASS / RECOVERED n / FAIL to
;                           gfxprobe_result.txt and End after resolution --
;                           lets scripts/gfxprobe_loop.ps1 measure the
;                           real-world incidence rate (run it with vs.
;                           without bin\dxgi.dll to test the ReShade theory)
;   RCCE_GFXPROBE=failexit  both.

Const RS_MAX_REINITS% = 2
Const RS_PROBE_SIZE%  = 32

; Result of the last EnsureRenderSanity call: 0 = clean first try,
; N>0 = recovered after N re-inits, -1 = still dead after all retries.
Global RenderSanityResult% = 0

; Last-probe measurements, consumed by RenderSanityWriteDiagnostic so the
; first-failure artifact can record exactly what the 2D and textured checks
; saw. Pixels are stored as raw ReadPixel ints (logged as 0xAARRGGBB hex).
Global RenderSanity2DOK%     = False
Global RenderSanity2DPixel%  = 0
Global RenderSanityTexOK%    = False
Global RenderSanityTexPixel% = 0

; One-shot guard: the first-failure artifact is written at most once per
; session so a flapping probe (e.g. RCCE_GFXPROBE=fail across every retry)
; can't spam the log directory.
Global RenderSanityDiagWritten% = False

; True when 2D image blits/text AND a textured 3D render both reach the
; backbuffer. Draws without ever Flipping, so the probe is never user-visible;
; cleans up after itself (frees the probe image/texture/entities, restores
; black ClsColor, clears the buffer).
Function RenderSanityProbe%()

	If Instr(GetEnv("RCCE_GFXPROBE"), "fail") > 0 Then Return False

	Local img.BBImage = CreateImage(RS_PROBE_SIZE, RS_PROBE_SIZE)
	If img = Null Then Return False

	; Fill the image magenta through its own buffer, then blit it onto a
	; black backbuffer next to a white text glyph. If the wrapper came up
	; with dead surfaces, none of these writes land and the scan below
	; reads pure black.
	SetBuffer ImageBuffer(img)
	ClsColor 255, 0, 255
	Cls
	SetBuffer BackBuffer()
	ClsColor 0, 0, 0
	Cls
	DrawBlock img, 0, 0
	Color 255, 255, 255
	Text RS_PROBE_SIZE + 8, 8, "RC"

	Local hit% = False
	Local x%, y%
	Local px% = 0
	For y = 2 To RS_PROBE_SIZE - 2 Step 6
		For x = 0 To RS_PROBE_SIZE + 24 Step 2
			px = ReadPixel(x, y)
			If (px And $FFFFFF) <> 0
				hit = True
				Exit
			EndIf
		Next
		If hit = True Then Exit
	Next

	FreeImage img
	Cls

	; Record the 2D result for the diagnostic artifact. px holds the first
	; non-black sample on success, or the last (black=0) sample on failure.
	RenderSanity2DOK    = hit
	RenderSanity2DPixel = px

	; A passing 2D check does NOT prove the texturing path works -- issue #40
	; is exactly 2D-OK + textures-dead. Require the textured check too.
	Local texOk% = RenderSanityTextureProbe()

	Return (hit And texOk)

End Function

; Textured-render check: build a throwaway 3D scene (one camera, one sprite
; textured with a known-green texture, full-bright so lighting can't tint it),
; RenderWorld it to the backbuffer (no Flip -- never user-visible), then
; ReadPixel the screen center and assert the texel came back GREEN. A sprite
; that renders but fails to sample its texture shows the white brush colour
; (untextured) or the black clear; the green-specific test rejects both, which
; is what distinguishes "textures work" from the #40 "untextured world".
; Everything created is freed before return. Returns True on a confirmed green
; texel, False on any failure (including resource-creation failure -- treated
; as "could not prove the texture path", i.e. unhealthy). Never RuntimeErrors.
Function RenderSanityTextureProbe%()

	If Instr(GetEnv("RCCE_GFXPROBE"), "fail") > 0 Then Return False

	RenderSanityTexOK    = False
	RenderSanityTexPixel = 0

	Local cam.BBEntity = CreateCamera()
	If cam = Null Then Return False
	CameraClsColor cam, 0, 0, 0
	PositionEntity cam, 0, 0, -5   ; look down +Z toward the sprite at the origin

	Local tex.BBTexture = CreateTexture(RS_PROBE_SIZE, RS_PROBE_SIZE)
	If tex = Null
		FreeEntity cam
		Return False
	EndIf
	SetBuffer TextureBuffer(tex)
	ClsColor 0, 255, 0
	Cls
	SetBuffer BackBuffer()

	Local spr.BBEntity = CreateSprite()
	If spr = Null
		FreeTexture tex
		FreeEntity cam
		Return False
	EndIf
	EntityTexture spr, tex
	EntityFX spr, 1                ; full-bright: texel colour is exact, lighting can't dim it
	ScaleSprite spr, 3, 3         ; enlarge so the screen centre lands solidly inside the quad

	RenderWorld

	; Sprite is centred at the origin and always faces the camera, so its
	; centre projects to the screen centre. Sample there and a few pixels
	; around it for tolerance.
	Local cx% = GraphicsWidth() / 2
	Local cy% = GraphicsHeight() / 2
	Local gotGreen% = False
	Local sample% = 0
	Local x%, y%
	Local p%, r%, g%, b%
	For y = cy - 4 To cy + 4 Step 2
		For x = cx - 4 To cx + 4 Step 2
			p = ReadPixel(x, y)
			r = (p Shr 16) And $FF
			g = (p Shr 8) And $FF
			b = p And $FF
			sample = p
			If g > 100 And r < 100 And b < 100
				gotGreen = True
				Exit
			EndIf
		Next
		If gotGreen = True Then Exit
	Next

	FreeEntity spr
	FreeTexture tex
	FreeEntity cam
	SetBuffer BackBuffer()
	ClsColor 0, 0, 0
	Cls

	RenderSanityTexOK    = gotGreen
	RenderSanityTexPixel = sample

	Return gotGreen

End Function

; Probe; on failure tear the graphics mode down and re-init with the SAME
; parameters (bounded), re-probing each time. Stores + returns the result
; code (0 clean / N recovered / -1 dead) -- callers log it with their own
; logger; this module's only module dependency is Logging.bb's atomic-write
; helpers (for the one-shot first-failure diagnostic).
Function EnsureRenderSanity%(w%, h%, d%, mode%)

	Local attempt%
	Local result% = -1
	For attempt = 0 To RS_MAX_REINITS
		If RenderSanityProbe() = True
			result = attempt
			Exit
		EndIf
		; First detected probe failure: capture a one-shot root-cause artifact
		; BEFORE the curative re-init perturbs device/swapchain state. The
		; function self-guards so it writes at most once per session even
		; though this line is reached on every failed attempt.
		RenderSanityWriteDiagnostic(w, h, d, mode, attempt)
		If attempt < RS_MAX_REINITS
			EndGraphics
			Graphics3D(w, h, d, mode)
			SetBuffer BackBuffer()
		EndIf
	Next

	RenderSanityResult = result
	RenderSanityReassertNotice()

	; Measurement mode: record + quit so a harness can tally incidence.
	; Anchor the result file to the EXECUTABLE's directory, not the working
	; directory -- every shipped target ChangeDirs to the project root
	; before this runs, while the harness looks next to the exe.
	If Instr(GetEnv("RCCE_GFXPROBE"), "exit") > 0
		Local rf.BBStream = WriteFile(SystemProperty("AppDir") + "gfxprobe_result.txt")
		If rf <> Null
			If result = 0
				WriteLine rf, "PASS"
			ElseIf result > 0
				WriteLine rf, "RECOVERED " + result
			Else
				WriteLine rf, "FAIL"
			EndIf
			CloseFile rf
		EndIf
		End
	EndIf

	Return result

End Function

; Write a one-shot first-failure diagnostic to the engine's log directory
; (Data\Logs\, the same dir Logging.bb's StartLog uses -- resolved relative to
; the working dir, which every target ChangeDirs to the project root before
; boot). Atomic via SafeWriteOpen/SafeWriteCommit so a crash mid-write leaves
; the previous artifact (if any) recoverable as .bak. Soft-fails everywhere:
; any inability to open/commit just returns False, never crashes the probe.
; Captures only what's cheaply available for diagnosing the dgVoodoo->ReShade
; swapchain race; APIs the runtime can't answer simply report 0/empty.
Function RenderSanityWriteDiagnostic%(w%, h%, d%, mode%, attempt%)

	; At most once per session.
	If RenderSanityDiagWritten = True Then Return False
	RenderSanityDiagWritten = True

	Local dir$ = "Data\Logs\"
	If FileType(dir$) <> 2 Then CreateDir dir$

	Local finalPath$ = dir$ + "gfxprobe_diag.txt"
	Local temp$ = SafeWriteOpen(finalPath$)

	Local df.BBStream = WriteFile(temp$)
	If df = Null Then Return False

	WriteLine df, "=== RCCE render-sanity diagnostic (issue #40) ==="
	WriteLine df, "when             : " + CurrentDate$() + " " + CurrentTime$()
	WriteLine df, "uptime_ms        : " + MilliSecs()
	WriteLine df, "retry_attempt    : " + attempt
	WriteLine df, "gfxprobe_env     : " + GetEnv("RCCE_GFXPROBE")
	WriteLine df, "requested_mode   : " + w + " x " + h + " x " + d + " (mode " + mode + ")"
	WriteLine df, "graphics_actual  : " + GraphicsWidth() + " x " + GraphicsHeight() + " x " + GraphicsDepth()
	WriteLine df, "vidmem_total     : " + TotalVidMem()
	WriteLine df, "vidmem_avail     : " + AvailVidMem()

	Local n% = CountGfxDrivers()
	WriteLine df, "gfx_driver_count : " + n
	Local i%
	For i = 1 To n
		WriteLine df, "gfx_driver[" + i + "]   : " + GfxDriverName$(i)
	Next

	WriteLine df, "probe_2d_ok      : " + RenderSanity2DOK
	WriteLine df, "probe_2d_pixel   : 0x" + Hex$(RenderSanity2DPixel)
	WriteLine df, "probe_tex_ok     : " + RenderSanityTexOK
	WriteLine df, "probe_tex_pixel  : 0x" + Hex$(RenderSanityTexPixel)

	; Cheap wrapper-presence signal next to the executable -- the dgVoodoo /
	; ReShade DLLs and their config sit alongside the exe, so their presence
	; tells the maintainer which wrapper stack was active for this hit.
	Local appdir$ = SystemProperty("AppDir")
	WriteLine df, "app_dir          : " + appdir$
	WriteLine df, "wrapper_dxgi_dll : " + RenderSanityFilePresent$(appdir$ + "dxgi.dll")
	WriteLine df, "wrapper_d3d9_dll : " + RenderSanityFilePresent$(appdir$ + "d3d9.dll")
	WriteLine df, "wrapper_d3d11_dll: " + RenderSanityFilePresent$(appdir$ + "d3d11.dll")
	WriteLine df, "reshade_ini      : " + RenderSanityFilePresent$(appdir$ + "ReShade.ini")
	WriteLine df, "dxgi_ini         : " + RenderSanityFilePresent$(appdir$ + "dxgi.ini")
	WriteLine df, "dgvoodoo_conf    : " + RenderSanityFilePresent$(appdir$ + "dgVoodoo.conf")

	; Close the handle ourselves (BBStream-typed under Strict), then commit
	; with F=0 so SafeWriteCommit just does the atomic .bak-demote + promote
	; -- its `If F <> 0 Then CloseFile(F)` guard makes 0 the "already closed"
	; contract, which sidesteps threading a BBStream through its int param.
	CloseFile df
	Return SafeWriteCommit(temp$, finalPath$, 0)

End Function

; True if a file exists at path (FileType 1 = file). Helper for the wrapper-
; presence probe in the diagnostic dump.
Function RenderSanityFilePresent$(path$)
	If FileType(path$) = 1 Then Return "yes (present)"
	Return "no"
End Function

; Re-assert the dead-surfaces title-bar notice. Title text is rendered by
; the OS, not DirectDraw, so it is readable precisely when nothing inside
; the window is -- but FUI_Initialise (GUE / Project Manager / Server) sets
; AppTitle unconditionally AFTER the probe runs, so those boot paths must
; call this again right after their F-UI init. No-op on healthy boots.
Function RenderSanityReassertNotice%()
	If RenderSanityResult >= 0 Then Return False
	AppTitle "RCCE: GRAPHICS BROKE (no textures - issue #40). Restart; if it recurs, remove bin\dxgi.dll"
	Return True
End Function
