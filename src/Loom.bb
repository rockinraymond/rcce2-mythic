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
// tracking that's its own design surface -- see docs/loom/decisions/002).
//
// Architecture: `Type Loom` owns instances of `Threads`, `Browser`, and
// `Composer`. The main loop calls `Loom::renderFrame(app)` once per frame.
// All three sub-modules are Types with Methods, called as
// `Module::method(self, args)` per the project's OO convention. See
// .claude/skills/blitzforge-language/SKILL.md "Module architecture" section.
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
// Per-tab dirty flags (shared with GUE). GUE declares the same set in GUE.bb
// at lines 47-48; declaring them here lets Loom write to them so the two
// editors see each other's dirty state. False = unsaved changes pending;
// True = on-disk == in-memory.
// -----------------------------------------------------------------------------
Global ItemsSaved = True
Global ActorsSaved = True
Global FactionsSaved = True
Global ParticlesSaved = True
Global DamageTypesSaved = True
Global ZoneSaved = True
Global AnimsSaved = True
Global StatsSaved = True
Global SpellsSaved = True
Global InterfaceSaved = True
Global ProjectilesSaved = True
Global EnvironmentSaved = True


// -----------------------------------------------------------------------------
// Includes
//
// Data layer: same modules GUE pulls in, minus UI-tied ones (F-UI,
// MediaDialogs, CharacterEditorLoader, ClientAreas). The loaders parse .dat
// files into the global type instances; Loom reads through those same
// instances so the two editors can't drift in how they parse the format.
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
// ClientAreas.bb deliberately omitted -- depends on GetFilename$ which lives
// inside GUE.bb. We don't need 3D zone meshes for the alpha; the composer
// renders zone metadata as text + portal chips. See
// docs/loom/decisions/004-deferred-3d-viewport.md.
Include "Modules\ServerAreas.bb"
Include "Modules\Packets.bb"
Include "Modules\Logging.bb"

// Loom UI layer.
Include "Modules\Loom\Theme.bb"
Include "Modules\Loom\Threads.bb"
Include "Modules\Loom\Browser.bb"
Include "Modules\Loom\Composer.bb"
Include "Modules\Loom\Palette.bb"


// =============================================================================
// Loom -- top-level application type. Owns the Threads / Browser / Composer
// instances and orchestrates the render loop.
// =============================================================================
Type Loom
    Field windowWidth%
    Field windowHeight%
    Field projectName$
    Field threads.Threads
    Field browser.Browser
    Field composer.Composer
    Field palette.Palette


    Method create.Loom(windowWidth%, windowHeight%, projectName$)
        self\windowWidth = windowWidth
        self\windowHeight = windowHeight
        self\projectName = projectName$

        // Shared focus + back stack
        self\threads = New Threads()

        // Browser, Composer, Palette all hold a reference to the same Threads
        // instance; card clicks call Threads::focus, chip + palette-result
        // clicks call Threads::jump.
        self\browser = New Browser(self\threads)
        self\composer = New Composer(self\threads)
        self\palette = New Palette(self\threads)

        Return self
    End Method


    // -------------------------------------------------------------------------
    // renderFrame -- paint browser, then composer overlay if focused, then
    // palette overlay if open, then process global keys. Returns False when
    // the user wants to exit. Returning a bool from the frame is cleaner than
    // mutating an `app\quit` field; the loop owns its own control flow.
    //
    // Render-order rationale: browser at the back, composer on top of
    // browser, palette on top of everything. Palette dims the world behind
    // itself so it visually owns the frame while open.
    //
    // Input-order rationale: Ctrl+K opens the palette BEFORE the palette's
    // own pumpKeyboard fires (so the K keystroke isn't appended to its
    // query). When the palette is open, its pumpKeyboard owns Esc; the outer
    // Esc handler below only runs when the palette is closed.
    // -------------------------------------------------------------------------
    Method renderFrame%()
        Cls

        // Ctrl+K opens the palette (or no-ops if already open). Detect
        // before any other input handler so openModal's FlushKeys swallows
        // the K keystroke before it can land in the palette query.
        If Palette::isOpen(self\palette) = False
            If (KeyDown(29) Or KeyDown(157)) And KeyHit(37)
                Palette::openModal(self\palette)
            EndIf
        EndIf

        // Browser input is enabled only when no higher-priority surface is
        // already consuming keystrokes. Priority chain (highest first):
        //   palette > composer-edit > browser filter
        Local browserInput% = True
        If Palette::isOpen(self\palette) = True Then browserInput = False
        If Composer::isEditing(self\composer) = True Then browserInput = False

        Browser::renderAndUpdate(self\browser, self\windowWidth, self\windowHeight, self\projectName, browserInput)
        Composer::renderAndUpdate(self\composer, self\windowWidth, self\windowHeight)

        // Palette consumes its own keys (including Esc) when open and
        // returns True to signal that.
        Local paletteAte% = Palette::renderAndUpdate(self\palette, self\windowWidth, self\windowHeight)

        // Esc priority (when palette didn't already eat the press):
        //   filter clear > back-stack pop > close composer > exit Loom
        If paletteAte = False And KeyHit(1)   // Esc
            If Browser::hasFilter(self\browser) = True
                Browser::clearFilter(self\browser)
            Else If Threads::back(self\threads) = False
                If self\threads\focusKind <> ""
                    // Close composer back to plain browser.
                    Threads::focus(self\threads, "", 0)
                    Threads::clearStack(self\threads)
                    WriteLog(LoomLog, "Esc: closed composer")
                Else
                    // Nothing left to close -- exit Loom.
                    Return False
                EndIf
            EndIf
        EndIf

        Flip
        Return True
    End Method
End Type


// =============================================================================
// Main -- bootstrap graphics, load data, build the Loom app, run frames.
// =============================================================================

// Graphics mode -- match GUE's window sizing so the two editors feel sibling.
Local boot_width# = GetSystemMetrics(0) * 0.9
Local boot_height# = GetSystemMetrics(1) * 0.8
If (boot_width < 1280 And boot_height < 800)
    boot_width = 1280
    boot_height = 800
EndIf

Graphics3D(boot_width, boot_height, 0, 2)
SetBuffer(BackBuffer())
AppTitle("Loom -- World Editor (Alpha) -- Realm Crafter " + rcceVersion$)

// Log -- Data\Logs\Loom Log.txt (relative to project root, next to GUE's log).
Global LoomLog = StartLog("Loom Log", False)
WriteLog(LoomLog, "** Loom startup begins **", True, True)
WriteLog(LoomLog, "Resolution: " + Str(boot_width) + "x" + Str(boot_height))

// Resolve project name from the working directory leaf.
Local cwd$ = CurrentDir$()
Local projectName$ = LoomGetLeafDir(cwd$)
WriteLog(LoomLog, "Project root: " + cwd$)
WriteLog(LoomLog, "Project name: " + projectName$)

LoomTheme_Init()


// -----------------------------------------------------------------------------
// Load project data. Same order GUE uses, same loaders, same in-memory
// representation. Failures RuntimeError with a Win32 dialog -- mirrors
// GUE.bb's behavior; a half-loaded project would just confuse the user later.
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
// Construct the app instance and run frames until renderFrame returns False.
// -----------------------------------------------------------------------------
Local app.Loom = New Loom(boot_width, boot_height, projectName)
WriteLog(LoomLog, "** Main loop running **")

While Loom::renderFrame(app) = True
Wend

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
