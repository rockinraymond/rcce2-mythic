// =============================================================================
// Loom.bb -- Loom World Editor (Alpha)
// =============================================================================
//
// A drop-in alternative to GUE, sharing the on-disk data formats but with
// a fresh UI built around the Loom design concept (see
// .claude/skills/loom-design-brief/ and the prototype handoff bundle).
//
// Architecture overview (the multi-PR roadmap; this commit ships only #1):
//
//   #1  Skeleton + theme + Project Manager launcher
//         Loom.exe compiles, Project Manager launches it,
//         shows a themed splash, exits cleanly. THIS COMMIT.
//
//   #2  Data loading + atlas
//         Loom uses GUE's existing data modules (Items.bb, Actors.bb,
//         Spells.bb, ServerAreas.bb, ...) via Include. After load,
//         the atlas surface lists every zone in the project.
//
//   #3  World view
//         Picking a zone in the atlas renders it in a 3D viewport
//         using Blitz3D's engine (the same engine GUE's Zones tab uses).
//         Click an entity to select it.
//
//   #4  Composer
//         Right-side property panel that paints the focused entity's
//         data (faction, level, mesh, equipped items) using Loom theme
//         primitives. Read-only for the alpha.
//
// Design intent for the alpha as a whole: "Loom can open my existing
// Realm Crafter project and let me look at my world through a different
// lens." Editing comes in beta.
// =============================================================================


// -----------------------------------------------------------------------------
// Bootstrap globals (mirrors GUE.bb's startup so the relative paths work
// identically -- both binaries live in bin/ and are launched with CWD set to
// <project>/Data/).
// -----------------------------------------------------------------------------
Global rcceVersion$ = "2.0.0"
Global componentName$ = "loom"
Global RootDir$ = "..\"

ChangeDir RootDir$


// -----------------------------------------------------------------------------
// Includes -- minimum surface for the skeleton.
//
// PR #1 deliberately does NOT include the data modules (Items, Actors,
// Spells, etc.) or F-UI. The skeleton's only job is to prove the build
// pipeline and the Project Manager hook work. PR #2 will add Logging-
// adjacent data loaders. PR #3 brings in Blitz3D's 3D pipeline for the
// world view.
// -----------------------------------------------------------------------------
Include "Modules\Logging.bb"
Include "Modules\Loom\Theme.bb"


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
// Log -- written to Data\Logs\Loom Log.txt (relative to project root, the
// same place GUE writes its log).
// -----------------------------------------------------------------------------
Global LoomLog = StartLog("Loom Log", False)
WriteLog(LoomLog, "** Loom startup begins **", True, True)
WriteLog(LoomLog, "Resolution: " + Str(Loom_width) + "x" + Str(Loom_height))


// -----------------------------------------------------------------------------
// Resolve project name from the working directory. When PM launches us, CWD
// has been set to <project>/Data/ and then ChangeDir "..\" walked us up to
// <project>/. The leaf folder name is the project's display name.
// -----------------------------------------------------------------------------
Local cwd$ = CurrentDir$()
Local projectName$ = LoomGetLeafDir(cwd$)
WriteLog(LoomLog, "Project root: " + cwd$)
WriteLog(LoomLog, "Project name: " + projectName$)

LoomTheme_Init()


// -----------------------------------------------------------------------------
// Splash screen loop. Runs until Esc.
// PR #2 replaces this with the atlas as the boot surface.
// -----------------------------------------------------------------------------
WriteLog(LoomLog, "** Splash loop running **")

Repeat
    Cls
    LoomRenderSplash(Loom_width, Loom_height, projectName$)
    Flip
Until KeyHit(1)

WriteLog(LoomLog, "** Loom shutdown **")
CloseAllLogs()
End


// =============================================================================
// LoomRenderSplash -- paint the alpha splash surface.
//
// Layout:
//   - Full-screen vertical gradient stone_900 -> stone_950
//   - Centered "LOOM" title in parchment
//   - "WORLD EDITOR" subtitle in brass, spaced
//   - Brass divider rule
//   - Project context line
//   - Footer instruction
// =============================================================================
Function LoomRenderSplash(sw, sh, projectName$)
    // Background gradient (BlitzForge does not support line continuation,
    // so these calls are intentionally long single lines.)
    LoomGradientV(0, 0, sw, sh, LOOM_STONE_900_R, LOOM_STONE_900_G, LOOM_STONE_900_B, LOOM_STONE_950_R, LOOM_STONE_950_G, LOOM_STONE_950_B)

    Local cx = sw / 2
    Local cy = sh / 2

    // Title -- "LOOM" centered, drawn twice with a 1px offset to fake bolder
    // weight on top of the Blitz default font. Real display fonts arrive in
    // a later PR.
    LoomTextCentered(cx, cy - 90, "LOOM", LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    LoomTextCentered(cx + 1, cy - 90, "LOOM", LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

    // Subtitle
    LoomTextCentered(cx, cy - 64, "W O R L D   E D I T O R", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

    // Brass divider (triple rule for an ornamented bar)
    LoomHRule(cx - 180, cy - 40, 360, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
    LoomHRule(cx - 180, cy - 39, 360, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomHRule(cx - 180, cy - 38, 360, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)

    // Project context
    LoomTextCentered(cx, cy - 16, "Alpha for " + projectName$, LOOM_STONE_200_R, LOOM_STONE_200_G, LOOM_STONE_200_B)
    LoomTextCentered(cx, cy + 2, "Realm Crafter Community Edition " + rcceVersion$, LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)

    // Skeleton-stage notice (will be removed in PR #2 when the atlas becomes
    // the boot surface).
    LoomTextCentered(cx, cy + 60, "skeleton build -- atlas, world view, and composer arrive in subsequent PRs", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)

    // Footer
    LoomTextCentered(cx, sh - 40, "Esc to exit", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)
End Function


// =============================================================================
// LoomGetLeafDir -- return the leaf folder name from a directory path.
// E.g. "C:\rcce2\projects\Embergloom" -> "Embergloom".
// Falls back to the whole path if no separator is found.
// =============================================================================
Function LoomGetLeafDir$(path$)
    Local trimmed$ = path$
    // Strip trailing slashes / backslashes so the leaf isn't an empty string.
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
