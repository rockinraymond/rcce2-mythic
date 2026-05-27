// =============================================================================
// Loom.bb -- Loom World Editor (Alpha)
// =============================================================================
//
// **Read docs/loom/README.md first** if you're picking this up. The north
// star, architecture, roadmap, and the ADRs that explain why the code is
// shaped the way it is all live under docs/loom/. The literal Claude Design
// prototype the alpha is built against is preserved at docs/loom/prototype/.
//
// A drop-in alternative to GUE, sharing the on-disk data formats but with
// a fresh UI built around the Loom design concept: every entity is browsable,
// every reference between entities is a clickable thread.
//
// Surface model:
//   BROWSER         everything-grid by category (Actors / Items / Spells /
//                   Zones / Factions / Animation Sets). Click a card to
//                   focus the entity in the COMPOSER.
//
//   COMPOSER        right-side property panel for the focused entity.
//                   Reference fields render as thread chips (Threads.bb);
//                   clicking a chip jumps focus and pushes a back-stack
//                   entry. Esc pops the stack (or, if empty, closes the
//                   composer back to the browser).
//
// Esc behavior:
//   composer focused, back stack non-empty   ->  pop one step back
//   composer focused, back stack empty       ->  close composer
//   browser only (nothing focused)           ->  exit Loom
//
// Read-only in this alpha. Editing is a beta concern (needs save/dirty
// tracking that's its own design surface).
// =============================================================================


// -----------------------------------------------------------------------------
// Bootstrap globals (mirrors GUE.bb so paths and log placement match -- both
// binaries live in bin/ and are launched from PM with CWD set to <proj>/Data).
// -----------------------------------------------------------------------------
Global rcceVersion$ = "2.0.0"
Global componentName$ = "loom"
Global RootDir$ = "..\"

ChangeDir RootDir$


// -----------------------------------------------------------------------------
// Includes
//
// Data layer: same modules GUE pulls in for the data layer, minus the
// UI-tied ones (F-UI, MediaDialogs, CharacterEditorLoader, ClientAreas).
// The loaders here parse .dat files into the global type instances; Loom
// reads through those same instances so the two editors can't drift apart
// in how they parse the files.
//
// Order matters for Type declarations -- mirror GUE.bb's order.
// -----------------------------------------------------------------------------
Include "Modules\RCEnet.bb"
Include "Modules\Media.bb"
Include "Modules\MediaImport.bb"
Include "Modules\Projectiles.bb"
Include "Modules\Language.bb"
Include "Modules\Items.bb"
Include "Modules\Inventories.bb"
Include "Modules\Animations.bb"
Include "Modules\Spells.bb"
Include "Modules\Actors.bb"
Include "Modules\Environment.bb"
Include "Modules\Interface.bb"
// ClientAreas.bb deliberately omitted -- depends on GetFilename$ which
// lives inside GUE.bb. We don't need 3D zone meshes for the alpha; the
// composer renders zone metadata as text + portal chips.
Include "Modules\ServerAreas.bb"
Include "Modules\Packets.bb"
Include "Modules\Logging.bb"

// Loom UI layer.
Include "Modules\Loom\Theme.bb"
Include "Modules\Loom\Threads.bb"
Include "Modules\Loom\Browser.bb"
Include "Modules\Loom\Composer.bb"


// -----------------------------------------------------------------------------
// Graphics mode -- match GUE's window sizing so the two editors feel sibling.
// -----------------------------------------------------------------------------
Local Loom_width# = GetSystemMetrics(0) * 0.9
Local Loom_height# = GetSystemMetrics(1) * 0.8
If (Loom_width < 1280 And Loom_height < 800)
    Loom_width = 1280
    Loom_height = 800
EndIf

Graphics3D(Loom_width, Loom_height, 0, 2)
SetBuffer(BackBuffer())
AppTitle("Loom -- World Editor (Alpha) -- Realm Crafter " + rcceVersion$)


// -----------------------------------------------------------------------------
// Log -- Data\Logs\Loom Log.txt (relative to project root, next to GUE's log).
// -----------------------------------------------------------------------------
Global LoomLog = StartLog("Loom Log", False)
WriteLog(LoomLog, "** Loom startup begins **", True, True)
WriteLog(LoomLog, "Resolution: " + Str(Loom_width) + "x" + Str(Loom_height))


// -----------------------------------------------------------------------------
// Resolve project name from the working directory leaf.
// -----------------------------------------------------------------------------
Local cwd$ = CurrentDir$()
Global LoomProjectName$ = LoomGetLeafDir(cwd$)
WriteLog(LoomLog, "Project root: " + cwd$)
WriteLog(LoomLog, "Project name: " + LoomProjectName$)

LoomTheme_Init()


// -----------------------------------------------------------------------------
// Load project data. Same order GUE uses, same loaders, same in-memory
// representation. Failures RuntimeError with a Win32 dialog -- mirrors
// GUE.bb's behavior; a half-loaded project would just confuse the user
// later.
// -----------------------------------------------------------------------------
WriteLog(LoomLog, "** Loading project data **")
Loom_DrawLoadingScreen("Loading project data...")

Loom_LoadStep("damage types", LoadDamageTypes("Data\Server Data\Damage.dat"), False)
Loom_LoadStep("attributes",   LoadAttributes("Data\Server Data\Attributes.dat"), False)
Loom_LoadStep("factions",     LoadFactions("Data\Server Data\Factions.dat"), True)
Loom_LoadStep("animations",   LoadAnimSets("Data\Game Data\Animations.dat"), True)

Global TotalProjectiles = LoadProjectiles("Data\Server Data\Projectiles.dat")
If TotalProjectiles = -1 Then RuntimeError("Loom could not open Data\Server Data\Projectiles.dat")
WriteLog(LoomLog, "Loaded " + Str(TotalProjectiles) + " projectiles")

Global TotalItems = LoadItems("Data\Server Data\Items.dat")
If TotalItems = -1 Then RuntimeError("Loom could not open Data\Server Data\Items.dat")
WriteLog(LoomLog, "Loaded " + Str(TotalItems) + " items")

Global TotalActors = LoadActors("Data\Server Data\Actors.dat")
If TotalActors = -1 Then RuntimeError("Loom could not open Data\Server Data\Actors.dat")
WriteLog(LoomLog, "Loaded " + Str(TotalActors) + " actors")

Global TotalSpells = LoadSpells("Data\Server Data\Spells.dat")
If TotalSpells = -1 Then RuntimeError("Loom could not open Data\Server Data\Spells.dat")
WriteLog(LoomLog, "Loaded " + Str(TotalSpells) + " spells")

Global TotalZones = 0
Local zoneDir = ReadDir("Data\Server Data\Areas")
Local zoneFile$ = NextFile$(zoneDir)
While zoneFile$ <> ""
    If FileType("Data\Server Data\Areas\" + zoneFile$) = 1 And Len(zoneFile$) > 4
        ServerLoadArea(Left$(zoneFile$, Len(zoneFile$) - 4))
        TotalZones = TotalZones + 1
    EndIf
    zoneFile$ = NextFile$(zoneDir)
Wend
CloseDir(zoneDir)
WriteLog(LoomLog, "Loaded " + Str(TotalZones) + " zones")

WriteLog(LoomLog, "** Data load complete **")


// -----------------------------------------------------------------------------
// Initialize Loom UI state.
// -----------------------------------------------------------------------------
Threads_Init()
Browser_Init()


// -----------------------------------------------------------------------------
// Main loop. Single surface that paints the browser, then layers the
// composer on top when something's focused. Click flows:
//
//   browser card click   -> Threads_Focus (no back-stack push)
//   composer chip click  -> Threads_Jump  (back-stack push)
//
// Esc consumes one of:
//   1. Threads_Back if back stack non-empty
//   2. Close composer if focus exists but stack empty
//   3. Exit Loom otherwise
// -----------------------------------------------------------------------------
WriteLog(LoomLog, "** Main loop running **")

Repeat
    Cls

    Browser_RenderAndUpdate(Loom_width, Loom_height, LoomProjectName$)
    Composer_RenderAndUpdate(Loom_width, Loom_height)

    If KeyHit(1)   // Esc
        If Threads_Back() = False
            If Loom_FocusKind$ <> ""
                // Close composer back to plain browser.
                Threads_Focus("", 0)
                Threads_ClearStack()
                WriteLog(LoomLog, "Esc: closed composer")
            Else
                // Nothing left to close -- exit Loom.
                Exit
            EndIf
        EndIf
    EndIf

    Flip
Until False

WriteLog(LoomLog, "** Loom shutdown **")
CloseAllLogs()
End


// =============================================================================
// Loom_LoadStep -- route the inconsistent loader return-value conventions
// (some return -1 on failure, some return False) through a single check.
// =============================================================================
Function Loom_LoadStep(stepName$, result, isMinusOneFailure)
    Local failed = False
    If isMinusOneFailure = True
        If result = -1 Then failed = True
    Else
        If result = False Then failed = True
    EndIf

    If failed = True
        WriteLog(LoomLog, "LOAD FAILED: " + stepName$)
        RuntimeError("Loom could not load " + stepName$ + ". Make sure the project's Data folder is intact and try again.")
    EndIf

    WriteLog(LoomLog, "Loaded " + stepName$)
End Function


// =============================================================================
// Loom_DrawLoadingScreen -- single-frame loading message while the data
// loaders run. The loads are fast enough on modern disks that an animated
// progress would just flicker.
// =============================================================================
Function Loom_DrawLoadingScreen(msg$)
    Cls
    LoomGradientV(0, 0, GraphicsWidth(), GraphicsHeight(), LOOM_STONE_900_R, LOOM_STONE_900_G, LOOM_STONE_900_B, LOOM_STONE_950_R, LOOM_STONE_950_G, LOOM_STONE_950_B)
    Local cx = GraphicsWidth() / 2
    Local cy = GraphicsHeight() / 2
    LoomTextCentered(cx, cy - 10, "LOOM", LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    LoomTextCentered(cx, cy + 10, msg$, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    Flip
End Function


// =============================================================================
// LoomGetLeafDir -- leaf folder name from a directory path.
// =============================================================================
Function LoomGetLeafDir$(path$)
    Local trimmed$ = path$
    While Len(trimmed$) > 1 And (Right$(trimmed$, 1) = "\" Or Right$(trimmed$, 1) = "/")
        trimmed$ = Left$(trimmed$, Len(trimmed$) - 1)
    Wend

    Local lastSep = 0
    Local i = 0
    For i = 1 To Len(trimmed$)
        Local ch$ = Mid$(trimmed$, i, 1)
        If ch$ = "\" Or ch$ = "/" Then lastSep = i
    Next

    If lastSep = 0 Then Return trimmed$
    Return Mid$(trimmed$, lastSep + 1)
End Function
