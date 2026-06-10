Strict

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
; This module turns that into: PROBE (do 2D surface blits actually reach the
; backbuffer?), bounded RE-INIT (EndGraphics + Graphics3D again, the same
; remedy a manual restart performs, up to RS_MAX_REINITS times), and REPORT
; (log-friendly return codes for the caller, plus an AppTitle notice on
; final failure -- the title bar is OS-rendered, NOT a DirectDraw surface,
; so it stays visible precisely when everything in the window is dead).
;
; Builtins only -- no module dependencies -- so every target (Client, GUE,
; Loom, Project Manager via RCCEGraphics) can include it. Call
; EnsureRenderSanity immediately after Graphics3D and BEFORE InitExt /
; FastExt setup, so a re-init never has to tear down cached device state.
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

; True when 2D image blits and text actually reach the backbuffer. Draws
; without ever Flipping, so the probe is never user-visible; cleans up after
; itself (frees the probe image, restores black ClsColor, clears the buffer).
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
	For y = 2 To RS_PROBE_SIZE - 2 Step 6
		For x = 0 To RS_PROBE_SIZE + 24 Step 2
			If (ReadPixel(x, y) And $FFFFFF) <> 0
				hit = True
				Exit
			EndIf
		Next
		If hit = True Then Exit
	Next

	FreeImage img
	Cls

	Return hit

End Function

; Probe; on failure tear the graphics mode down and re-init with the SAME
; parameters (bounded), re-probing each time. Stores + returns the result
; code (0 clean / N recovered / -1 dead) -- callers log it with their own
; logger; this module deliberately has no logging dependency.
Function EnsureRenderSanity%(w%, h%, d%, mode%)

	Local attempt%
	Local result% = -1
	For attempt = 0 To RS_MAX_REINITS
		If RenderSanityProbe() = True
			result = attempt
			Exit
		EndIf
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
