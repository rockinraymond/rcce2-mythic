// =============================================================================
// Loom/Composer.bb -- per-kind detail page for the focused entity
// =============================================================================
//
// When the user picks an entity in the browser (or follows a thread chip),
// the composer slides in from the right and paints that entity's
// properties. Each kind has its own field layout. Reference fields render
// as thread chips (via Threads.bb) -- clicking a chip jumps and pushes the
// current focus onto the back stack.
//
// Reads:
//   Loom_FocusKind$ / Loom_FocusID  (from Threads.bb)
//   the underlying data modules' globals (ActorList, ItemList, SpellsList,
//   Each Area, FactionNames$, Each AnimSet)
//
// Writes:
//   Nothing -- composer is read-only in the alpha.
//
// Per-kind field surface (only what's load-bearing for the alpha):
//
//   actor    : Race/Class, Description, Faction chip, M/F AnimSet chips,
//              Aggressiveness, Genders, Playable, Rideable, XP multiplier
//   item     : Name, Type, Slot, Value, Mass, Weapon damage / Armour level
//              (kind-specific), exclusive race/class, script binding
//   spell    : Name, Description, Recharge, exclusive race/class, script
//   zone     : Name, Outdoors, PvP, Gravity, entry/exit scripts, summary
//              counts, portal list with target-zone chips
//   faction  : Name, member roster (chips to every actor with this faction)
//   animset  : Name, clip count, used-by roster (chips to every actor using
//              this anim set as M or F animation)
//
// Public API:
//   Composer_Width()
//     Returns 0 when no focus, else the composer's pixel width. Browser
//     uses this to dim its background (and future PRs could shrink the
//     grid to make room).
//
//   Composer_RenderAndUpdate(sw, sh) -> True if a chip was clicked
//     Per-frame paint + chip hit-test. Returns True if any thread chip
//     was clicked (the jump has already happened by then, via the chip's
//     own click handler).
// =============================================================================


Const CMP_W           = 380
Const CMP_TOP         = 56     // matches BR_TOP_RIBBON / ZM_TOP_RIBBON
Const CMP_BOT_PAD     = 36     // matches BR_BOT_RIBBON
Const CMP_PAD         = 16
Const CMP_ROW_H       = 22
Const CMP_CHIP_H      = 26


// =============================================================================
// Composer_Width -- 0 when nothing's focused.
// =============================================================================
Function Composer_Width()
    If Loom_FocusKind$ = "" Then Return 0
    Return CMP_W
End Function


// =============================================================================
// Composer_RenderAndUpdate -- per-frame, when something's focused.
// =============================================================================
Function Composer_RenderAndUpdate(sw, sh)
    If Loom_FocusKind$ = "" Then Return False

    Local mx = MouseX()
    Local my = MouseY()
    Local clicked = MouseHit(1)

    Local x = sw - CMP_W
    Local y = CMP_TOP
    Local w = CMP_W
    Local h = sh - CMP_TOP - CMP_BOT_PAD

    // Panel chrome -- brass left rule signals the primary surface
    LoomFill(x, y, w, h, LOOM_STONE_850_R, LOOM_STONE_850_G, LOOM_STONE_850_B)
    LoomBorder(x, y, w, h, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
    LoomFill(x, y, 3, h, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

    // Title block
    Local kindLabel$ = Composer_KindLabel$(Loom_FocusKind$)
    Local name$ = Threads_LookupName$(Loom_FocusKind$, Loom_FocusID)
    If name$ = "" Then name$ = "(unknown)"

    LoomText(x + CMP_PAD, y + CMP_PAD, kindLabel$, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomText(x + CMP_PAD, y + CMP_PAD + 16, name$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    LoomHRule(x + CMP_PAD, y + CMP_PAD + 38, w - CMP_PAD * 2, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)

    // Body
    Local bodyY = y + CMP_PAD + 50
    Local bodyH = h - (bodyY - y) - 24
    Local clickedAChip = False

    If Loom_FocusKind$ = "actor"
        clickedAChip = Composer_RenderActor(x, bodyY, w, bodyH, mx, my, clicked)
    Else If Loom_FocusKind$ = "item"
        clickedAChip = Composer_RenderItem(x, bodyY, w, bodyH, mx, my, clicked)
    Else If Loom_FocusKind$ = "spell"
        clickedAChip = Composer_RenderSpell(x, bodyY, w, bodyH, mx, my, clicked)
    Else If Loom_FocusKind$ = "zone"
        clickedAChip = Composer_RenderZone(x, bodyY, w, bodyH, mx, my, clicked)
    Else If Loom_FocusKind$ = "faction"
        clickedAChip = Composer_RenderFaction(x, bodyY, w, bodyH, mx, my, clicked)
    Else If Loom_FocusKind$ = "animset"
        clickedAChip = Composer_RenderAnimSet(x, bodyY, w, bodyH, mx, my, clicked)
    EndIf

    // Footer: back-stack hint
    Local stackSize = ListSize(Loom_BackStack)
    Local footMsg$ = "Esc returns to browser"
    If stackSize > 0
        footMsg$ = "Esc walks back  ·  " + Str(stackSize) + " in trail"
    EndIf
    LoomText(x + CMP_PAD, y + h - 22, footMsg$, LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)

    Return clickedAChip
End Function


// -----------------------------------------------------------------------------
// Layout helpers
// -----------------------------------------------------------------------------

// label : value row. Label brass, value parchment. Returns the next Y.
Function Composer_Row(panelX, panelW, rowY, label$, value$)
    LoomText(panelX + CMP_PAD,        rowY, label$, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomText(panelX + CMP_PAD + 120,  rowY, value$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    Return rowY + CMP_ROW_H
End Function


// label : thread chip row. Returns the next Y + whether the chip was clicked.
// Blitz has no out-params; we OR-fold into the global Composer_ChipHit flag.
Global Composer_ChipHit = False
Function Composer_ChipRow(panelX, panelW, rowY, label$, kind$, refID, mx, my, clicked)
    LoomText(panelX + CMP_PAD, rowY + 4, label$, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

    Local chipX = panelX + CMP_PAD + 120
    Local chipW = panelW - CMP_PAD * 2 - 120
    Local hit = Threads_RenderChip(chipX, rowY, chipW, CMP_CHIP_H, kind$, refID, mx, my, clicked)
    If hit Then Composer_ChipHit = True

    Return rowY + CMP_CHIP_H + 4
End Function


// Section header inside a panel body. Used to break long composer pages into
// labeled groups (e.g. "Members" header in faction composer).
Function Composer_SectionHeader(panelX, panelW, rowY, title$)
    LoomHRule(panelX + CMP_PAD, rowY + 6, panelW - CMP_PAD * 2, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
    LoomText(panelX + CMP_PAD, rowY + 10, title$, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    Return rowY + 28
End Function


Function Composer_KindLabel$(kind$)
    If kind$ = "actor"   Then Return "ACTOR"
    If kind$ = "item"    Then Return "ITEM"
    If kind$ = "spell"   Then Return "SPELL"
    If kind$ = "zone"    Then Return "ZONE"
    If kind$ = "faction" Then Return "FACTION"
    If kind$ = "animset" Then Return "ANIMATION SET"
    Return Upper$(kind$)
End Function


// =============================================================================
// Per-kind body renderers
// =============================================================================

Function Composer_RenderActor(panelX, bodyY, panelW, bodyH, mx, my, clicked)
    Composer_ChipHit = False
    If Loom_FocusID < 0 Or Loom_FocusID > 65535 Then Return False
    Local A.Actor = ActorList(Loom_FocusID)
    If A = Null Then Return False

    Local y = bodyY
    y = Composer_Row(panelX, panelW, y, "ID",            Str(A\ID))
    y = Composer_Row(panelX, panelW, y, "Race",          A\Race$)
    y = Composer_Row(panelX, panelW, y, "Class",         A\Class$)
    y = Composer_Row(panelX, panelW, y, "Aggressiveness",Composer_ActorAggLabel$(A\Aggressiveness))
    y = Composer_Row(panelX, panelW, y, "Genders",       Composer_ActorGenderLabel$(A\Genders))
    y = Composer_Row(panelX, panelW, y, "Playable",      Composer_BoolLabel$(A\Playable))
    y = Composer_Row(panelX, panelW, y, "Rideable",      Composer_BoolLabel$(A\Rideable))
    y = Composer_Row(panelX, panelW, y, "XP multiplier", Str(A\XPMultiplier))

    y = Composer_SectionHeader(panelX, panelW, y, "Threads")

    y = Composer_ChipRow(panelX, panelW, y, "Faction",        "faction", A\DefaultFaction, mx, my, clicked)
    y = Composer_ChipRow(panelX, panelW, y, "M anim set",     "animset", A\MAnimationSet,  mx, my, clicked)
    y = Composer_ChipRow(panelX, panelW, y, "F anim set",     "animset", A\FAnimationSet,  mx, my, clicked)

    Return Composer_ChipHit
End Function


Function Composer_RenderItem(panelX, bodyY, panelW, bodyH, mx, my, clicked)
    Composer_ChipHit = False
    If Loom_FocusID < 0 Or Loom_FocusID > 65534 Then Return False
    Local It.Item = ItemList(Loom_FocusID)
    If It = Null Then Return False

    Local y = bodyY
    y = Composer_Row(panelX, panelW, y, "ID",         Str(It\ID))
    y = Composer_Row(panelX, panelW, y, "Type",       Browser_ItemTypeLabel$(It\ItemType))
    y = Composer_Row(panelX, panelW, y, "Slot",       Str(It\SlotType))
    y = Composer_Row(panelX, panelW, y, "Value",      Str(It\Value))
    y = Composer_Row(panelX, panelW, y, "Mass",       Str(It\Mass))
    y = Composer_Row(panelX, panelW, y, "Stackable",  Composer_BoolLabel$(It\Stackable))
    y = Composer_Row(panelX, panelW, y, "Breakable",  Composer_BoolLabel$(It\TakesDamage))

    // Weapon-specific
    If It\ItemType = 1
        y = Composer_SectionHeader(panelX, panelW, y, "Weapon")
        y = Composer_Row(panelX, panelW, y, "Damage",      Str(It\WeaponDamage))
        y = Composer_Row(panelX, panelW, y, "Weapon type", Str(It\WeaponType))
        If It\Range# > 0.0
            y = Composer_Row(panelX, panelW, y, "Range",   Composer_FormatFloat$(It\Range#))
        EndIf
    EndIf

    // Armour-specific
    If It\ItemType = 2
        y = Composer_SectionHeader(panelX, panelW, y, "Armour")
        y = Composer_Row(panelX, panelW, y, "Armour level", Str(It\ArmourLevel))
    EndIf

    // Restrictions
    If It\ExclusiveRace$ <> "" Or It\ExclusiveClass$ <> ""
        y = Composer_SectionHeader(panelX, panelW, y, "Restricted to")
        If It\ExclusiveRace$ <> ""
            y = Composer_Row(panelX, panelW, y, "Race",  It\ExclusiveRace$)
        EndIf
        If It\ExclusiveClass$ <> ""
            y = Composer_Row(panelX, panelW, y, "Class", It\ExclusiveClass$)
        EndIf
    EndIf

    // Script
    If It\Script$ <> ""
        y = Composer_SectionHeader(panelX, panelW, y, "Script")
        y = Composer_Row(panelX, panelW, y, "Bound",  It\Script$)
        If It\SMethod$ <> ""
            y = Composer_Row(panelX, panelW, y, "Method", It\SMethod$)
        EndIf
    EndIf

    Return Composer_ChipHit
End Function


Function Composer_RenderSpell(panelX, bodyY, panelW, bodyH, mx, my, clicked)
    Composer_ChipHit = False
    If Loom_FocusID < 0 Or Loom_FocusID > 65534 Then Return False
    Local S.Spell = SpellsList(Loom_FocusID)
    If S = Null Then Return False

    Local y = bodyY
    y = Composer_Row(panelX, panelW, y, "ID",          Str(S\ID))
    y = Composer_Row(panelX, panelW, y, "Recharge",    Str(S\RechargeTime) + " ms")

    If S\Description$ <> ""
        y = Composer_SectionHeader(panelX, panelW, y, "Description")
        // Description can be long; clip to one line. Future PR: word-wrap.
        Local desc$ = S\Description$
        If Len(desc$) > 60 Then desc$ = Left$(desc$, 57) + "..."
        LoomText(panelX + CMP_PAD, y, desc$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
        y = y + CMP_ROW_H + 4
    EndIf

    If S\ExclusiveRace$ <> "" Or S\ExclusiveClass$ <> ""
        y = Composer_SectionHeader(panelX, panelW, y, "Restricted to")
        If S\ExclusiveRace$  <> "" Then y = Composer_Row(panelX, panelW, y, "Race",  S\ExclusiveRace$)
        If S\ExclusiveClass$ <> "" Then y = Composer_Row(panelX, panelW, y, "Class", S\ExclusiveClass$)
    EndIf

    If S\Script$ <> ""
        y = Composer_SectionHeader(panelX, panelW, y, "Script")
        y = Composer_Row(panelX, panelW, y, "Bound",  S\Script$)
        If S\SMethod$ <> "" Then y = Composer_Row(panelX, panelW, y, "Method", S\SMethod$)
    EndIf

    Return Composer_ChipHit
End Function


Function Composer_RenderZone(panelX, bodyY, panelW, bodyH, mx, my, clicked)
    Composer_ChipHit = False
    Local Ar.Area = Object.Area(Loom_FocusID)
    If Ar = Null Then Return False

    Local y = bodyY
    y = Composer_Row(panelX, panelW, y, "Name",     Ar\Name$)
    y = Composer_Row(panelX, panelW, y, "Outdoors", Composer_BoolLabel$(Ar\Outdoors))
    y = Composer_Row(panelX, panelW, y, "PvP",      Composer_BoolLabel$(Ar\PvP))
    y = Composer_Row(panelX, panelW, y, "Gravity",  Str(Ar\Gravity))

    // Counts
    Local portals = 0, spawns = 0, triggers = 0, waypoints = 0
    Local i = 0
    For i = 0 To 99
        If Ar\PortalName$[i] <> "" Then portals = portals + 1
    Next
    For i = 0 To 999
        If Ar\SpawnActor[i] > 0 Then spawns = spawns + 1
    Next
    For i = 0 To 149
        If Ar\TriggerScript$[i] <> "" Then triggers = triggers + 1
    Next
    For i = 0 To 1999
        If Ar\WaypointX#[i] <> 0.0 Or Ar\WaypointZ#[i] <> 0.0 Then waypoints = waypoints + 1
    Next

    y = Composer_SectionHeader(panelX, panelW, y, "Contents")
    y = Composer_Row(panelX, panelW, y, "Portals",   Str(portals))
    y = Composer_Row(panelX, panelW, y, "Spawns",    Str(spawns))
    y = Composer_Row(panelX, panelW, y, "Triggers",  Str(triggers))
    y = Composer_Row(panelX, panelW, y, "Waypoints", Str(waypoints))

    // Scripts
    If Ar\EntryScript$ <> "" Or Ar\ExitScript$ <> ""
        y = Composer_SectionHeader(panelX, panelW, y, "Scripts")
        If Ar\EntryScript$ <> "" Then y = Composer_Row(panelX, panelW, y, "Entry", Ar\EntryScript$)
        If Ar\ExitScript$  <> "" Then y = Composer_Row(panelX, panelW, y, "Exit",  Ar\ExitScript$)
    EndIf

    // Portal links -- one chip per portal whose target resolves to a zone
    // we know about. Builds the most-useful thread set zones can offer.
    If portals > 0
        y = Composer_SectionHeader(panelX, panelW, y, "Portal links")
        Local p = 0
        For p = 0 To 99
            If Ar\PortalName$[p] <> "" And y < bodyY + bodyH - CMP_CHIP_H - 24
                Local targetHandle = Composer_FindZoneByName(Ar\PortalLinkArea$[p])
                If targetHandle <> 0
                    y = Composer_ChipRow(panelX, panelW, y, Ar\PortalName$[p], "zone", targetHandle, mx, my, clicked)
                Else
                    // Unknown target -- render a brass label that says where it points
                    LoomText(panelX + CMP_PAD, y + 4, Ar\PortalName$[p], LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
                    Local tgt$ = Ar\PortalLinkArea$[p]
                    If tgt$ = "" Then tgt$ = "(no target)"
                    LoomText(panelX + CMP_PAD + 120, y + 4, tgt$, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
                    y = y + CMP_ROW_H
                EndIf
            EndIf
        Next
    EndIf

    Return Composer_ChipHit
End Function


Function Composer_RenderFaction(panelX, bodyY, panelW, bodyH, mx, my, clicked)
    Composer_ChipHit = False
    Local idx = Loom_FocusID
    If idx < 0 Or idx > 99 Then Return False

    Local y = bodyY
    y = Composer_Row(panelX, panelW, y, "Name",  FactionNames$(idx))
    y = Composer_Row(panelX, panelW, y, "Index", Str(idx))

    // Members -- every actor whose DefaultFaction matches this index.
    // Each member renders as an actor chip. Capped to whatever fits in
    // the panel body (no scrolling yet).
    y = Composer_SectionHeader(panelX, panelW, y, "Members")

    Local memberCount = 0
    For Ac.Actor = Each Actor
        If Ac\DefaultFaction = idx And y < bodyY + bodyH - CMP_CHIP_H - 24
            y = Composer_ChipRow(panelX, panelW, y, "", "actor", Ac\ID, mx, my, clicked)
            memberCount = memberCount + 1
        EndIf
    Next

    If memberCount = 0
        LoomText(panelX + CMP_PAD, y + 4, "(no members)", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)
    EndIf

    Return Composer_ChipHit
End Function


Function Composer_RenderAnimSet(panelX, bodyY, panelW, bodyH, mx, my, clicked)
    Composer_ChipHit = False
    Local targetID = Loom_FocusID

    // AnimSet is iterated, not indexed -- walk to find.
    Local A.AnimSet = Null
    For As.AnimSet = Each AnimSet
        If As\ID = targetID Then A = As : Exit
    Next
    If A = Null Then Return False

    Local y = bodyY
    y = Composer_Row(panelX, panelW, y, "Name", A\Name$)
    y = Composer_Row(panelX, panelW, y, "ID",   Str(A\ID))

    Local clips = 0
    Local i = 0
    For i = 0 To 149
        If A\AnimName$[i] <> "" Then clips = clips + 1
    Next
    y = Composer_Row(panelX, panelW, y, "Clips", Str(clips))

    y = Composer_SectionHeader(panelX, panelW, y, "Used by")
    Local userCount = 0
    For Ac.Actor = Each Actor
        If (Ac\MAnimationSet = targetID Or Ac\FAnimationSet = targetID) And y < bodyY + bodyH - CMP_CHIP_H - 24
            y = Composer_ChipRow(panelX, panelW, y, "", "actor", Ac\ID, mx, my, clicked)
            userCount = userCount + 1
        EndIf
    Next

    If userCount = 0
        LoomText(panelX + CMP_PAD, y + 4, "(no users)", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)
    EndIf

    Return Composer_ChipHit
End Function


// =============================================================================
// Value formatters
// =============================================================================

Function Composer_BoolLabel$(b)
    If b Then Return "Yes"
    Return "No"
End Function

Function Composer_FormatFloat$(v#)
    Local rounded# = Float(Int(v# * 10.0)) / 10.0
    Return Str$(rounded#)
End Function

Function Composer_ActorAggLabel$(a)
    If a = 0 Then Return "Passive"
    If a = 1 Then Return "Defensive"
    If a = 2 Then Return "Always attacks"
    If a = 3 Then Return "Non-combatant"
    Return Str(a)
End Function

Function Composer_ActorGenderLabel$(g)
    If g = 0 Then Return "Both"
    If g = 1 Then Return "Male only"
    If g = 2 Then Return "Female only"
    If g = 3 Then Return "No gender"
    Return Str(g)
End Function


// Resolve a zone name to its Handle. Returns 0 if not found.
// Used by zone composer to wire portal targets to thread chips.
Function Composer_FindZoneByName(name$)
    If name$ = "" Then Return 0
    For Ar.Area = Each Area
        If Upper$(Ar\Name$) = Upper$(name$) Then Return Handle(Ar)
    Next
    Return 0
End Function
