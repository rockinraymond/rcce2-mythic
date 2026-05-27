// =============================================================================
// Loom/Browser.bb -- everything-browser (entity picker grid by category)
// =============================================================================
//
// The boot surface. Replaces the zone-only atlas: every entity type the
// project contains gets its own category, and each category renders as a
// grid of clickable cards. Clicking a card focuses the entity in the
// composer (which slides in from the right via Composer.bb).
//
// Categories (in tab order):
//   actor / item / spell / zone / faction / animset
//
// Per-category card content (kept compact so 3 cards fit per row at 1280):
//   actor   : "Race [Class]" + faction name + level
//   item    : name + type label (Weapon / Armour / Potion / etc.) + value
//   spell   : name + "Recharge Ns" subtitle
//   zone    : name + portal/spawn/trigger counts (same as old zone atlas)
//   faction : name + member count (computed: actors whose DefaultFaction
//             equals this faction index)
//   animset : name + animation-clip count
//
// Public API:
//   Browser_Init()
//     One-time setup. Picks "actor" as the initial category and pre-builds
//     the category-bar tab rects (recomputed each frame from sw, kept here
//     so the constants are defined in one place).
//
//   Browser_RenderAndUpdate(sw, sh, project$) -> True if a card was clicked
//     Per-frame paint + hit-test. Returns True when the user clicked a
//     card; the click set Loom_FocusKind$/Loom_FocusID via Threads_Focus
//     already, so the caller just needs to switch into compose mode.
//
//   Browser_Categories[] -- the ordered list of category kinds, exposed
//     so the chrome can iterate.
// =============================================================================


// Layout
Const BR_TOP_RIBBON  = 56
Const BR_TAB_BAR_H   = 36
Const BR_BOT_RIBBON  = 36
Const BR_SECTION_PAD = 28
Const BR_CARD_W      = 300
Const BR_CARD_H      = 96
Const BR_CARD_GAP    = 14


// Category state. Initialized to "actor" because actors are usually the
// richest content in a project and the most useful starting point.
Global Browser_Category$ = "actor"


// Category descriptors -- title shown in the tab bar + the kind id used
// across the rest of the Loom code. Iterated via `Each BrowserCategory`
// in insertion order (Blitz3D's global type pool is FIFO), which is also
// the tab-bar order.
Type BrowserCategory
    Field Kind$
    Field Title$
End Type


// =============================================================================
// Browser_Init
// =============================================================================
Function Browser_Init()
    // Build the ordered category list once.
    Browser_AddCategory("actor",   "Actors")
    Browser_AddCategory("item",    "Items")
    Browser_AddCategory("spell",   "Spells")
    Browser_AddCategory("zone",    "Zones")
    Browser_AddCategory("faction", "Factions")
    Browser_AddCategory("animset", "Animation Sets")
End Function


Function Browser_AddCategory(kind$, title$)
    Local c.BrowserCategory = New BrowserCategory
    c\Kind$ = kind$
    c\Title$ = title$
End Function


// =============================================================================
// Browser_RenderAndUpdate -- per-frame paint + hit-test.
// =============================================================================
Function Browser_RenderAndUpdate(sw, sh, project$)
    Local mx = MouseX()
    Local my = MouseY()
    Local clicked = MouseHit(1)

    // Background gradient
    LoomGradientV(0, 0, sw, sh, LOOM_STONE_900_R, LOOM_STONE_900_G, LOOM_STONE_900_B, LOOM_STONE_950_R, LOOM_STONE_950_G, LOOM_STONE_950_B)

    // Chrome
    Browser_DrawTopRibbon(sw, project$)
    Browser_DrawTabBar(sw, mx, my, clicked)
    Browser_DrawFooter(sw, sh)

    // Card grid
    Local clickedACard = Browser_DrawCardGrid(sw, sh, mx, my, clicked)
    Return clickedACard
End Function


// -----------------------------------------------------------------------------
// Top brand strip
// -----------------------------------------------------------------------------
Function Browser_DrawTopRibbon(sw, project$)
    LoomFill(0, 0, sw, BR_TOP_RIBBON, LOOM_STONE_850_R, LOOM_STONE_850_G, LOOM_STONE_850_B)
    LoomHRule(0, BR_TOP_RIBBON - 1, sw, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
    LoomHRule(0, BR_TOP_RIBBON,     sw, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomHRule(0, BR_TOP_RIBBON + 1, sw, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)

    LoomText(20, 18, "LOOM",    LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    LoomText(20, 32, "Browser", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

    LoomTextCentered(sw / 2, 22, project$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
End Function


// -----------------------------------------------------------------------------
// Category tab bar -- one tab per kind, active tab gets a brass underline.
// Hit-test is inline -- no side-table needed.
// -----------------------------------------------------------------------------
Function Browser_DrawTabBar(sw, mx, my, clicked)
    Local y = BR_TOP_RIBBON
    Local h = BR_TAB_BAR_H
    LoomFill(0, y, sw, h, LOOM_STONE_800_R, LOOM_STONE_800_G, LOOM_STONE_800_B)
    LoomHRule(0, y + h, sw, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)

    Local x = 20
    For c.BrowserCategory = Each BrowserCategory
        // Pad each tab label so the click target is generous.
        Local w = StringWidth(c\Title$) + 40
        Local active = (c\Kind$ = Browser_Category$)
        Local hovered = (mx >= x And mx < x + w And my >= y And my < y + h)

        // Hover background
        If hovered = True
            LoomFill(x, y, w, h, LOOM_STONE_700_R, LOOM_STONE_700_G, LOOM_STONE_700_B)
        EndIf

        // Tab label color
        If active = True
            LoomText(x + 20, y + 11, c\Title$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
            // Brass underline for the active tab
            LoomFill(x + 8, y + h - 3, w - 16, 3, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        Else
            LoomText(x + 20, y + 11, c\Title$, LOOM_STONE_200_R, LOOM_STONE_200_G, LOOM_STONE_200_B)
        EndIf

        // Hit-test
        If hovered And clicked
            Browser_Category$ = c\Kind$
            WriteLog(LoomLog, "Browser: category -> " + c\Kind$)
        EndIf

        x = x + w + 6
    Next
End Function


// -----------------------------------------------------------------------------
// Card grid -- one card per entity in the current category. Hit-test is
// inline in Browser_DrawCardChrome; returns True if a card was clicked
// this frame (and as a side effect Threads_Focus has been called).
// -----------------------------------------------------------------------------
Function Browser_DrawCardGrid(sw, sh, mx, my, clicked)
    Local gridX = BR_SECTION_PAD
    Local gridY = BR_TOP_RIBBON + BR_TAB_BAR_H + BR_SECTION_PAD
    Local gridW = sw - (BR_SECTION_PAD * 2)
    Local cols = (gridW + BR_CARD_GAP) / (BR_CARD_W + BR_CARD_GAP)
    If cols < 1 Then cols = 1

    Local col = 0
    Local row = 0
    Local count = 0
    Local clickedACard = False

    // Iterate the in-memory store for the current category and render one
    // card per entity. We don't precompute the list because data could
    // change between frames (no actual edit yet, but future-proofing).

    If Browser_Category$ = "actor"
        For Ac.Actor = Each Actor
            Local cx1 = gridX + col * (BR_CARD_W + BR_CARD_GAP)
            Local cy1 = gridY + row * (BR_CARD_H + BR_CARD_GAP)
            If cy1 + BR_CARD_H < sh - BR_BOT_RIBBON
                Local h1 = Browser_DrawCardChrome("actor", Ac\ID, cx1, cy1, mx, my, clicked)
                Browser_DrawActorCardBody(Ac, cx1, cy1)
                If h1 Then clickedACard = True
            EndIf
            count = count + 1 : col = col + 1
            If col >= cols Then col = 0 : row = row + 1
        Next
    Else If Browser_Category$ = "item"
        For It.Item = Each Item
            Local cx2 = gridX + col * (BR_CARD_W + BR_CARD_GAP)
            Local cy2 = gridY + row * (BR_CARD_H + BR_CARD_GAP)
            If cy2 + BR_CARD_H < sh - BR_BOT_RIBBON
                Local h2 = Browser_DrawCardChrome("item", It\ID, cx2, cy2, mx, my, clicked)
                Browser_DrawItemCardBody(It, cx2, cy2)
                If h2 Then clickedACard = True
            EndIf
            count = count + 1 : col = col + 1
            If col >= cols Then col = 0 : row = row + 1
        Next
    Else If Browser_Category$ = "spell"
        For Sp.Spell = Each Spell
            Local cx3 = gridX + col * (BR_CARD_W + BR_CARD_GAP)
            Local cy3 = gridY + row * (BR_CARD_H + BR_CARD_GAP)
            If cy3 + BR_CARD_H < sh - BR_BOT_RIBBON
                Local h3 = Browser_DrawCardChrome("spell", Sp\ID, cx3, cy3, mx, my, clicked)
                Browser_DrawSpellCardBody(Sp, cx3, cy3)
                If h3 Then clickedACard = True
            EndIf
            count = count + 1 : col = col + 1
            If col >= cols Then col = 0 : row = row + 1
        Next
    Else If Browser_Category$ = "zone"
        For Ar.Area = Each Area
            Local cx4 = gridX + col * (BR_CARD_W + BR_CARD_GAP)
            Local cy4 = gridY + row * (BR_CARD_H + BR_CARD_GAP)
            If cy4 + BR_CARD_H < sh - BR_BOT_RIBBON
                Local h4 = Browser_DrawCardChrome("zone", Handle(Ar), cx4, cy4, mx, my, clicked)
                Browser_DrawZoneCardBody(Ar, cx4, cy4)
                If h4 Then clickedACard = True
            EndIf
            count = count + 1 : col = col + 1
            If col >= cols Then col = 0 : row = row + 1
        Next
    Else If Browser_Category$ = "faction"
        Local i = 0
        For i = 0 To 99
            If FactionNames$(i) <> ""
                Local cx5 = gridX + col * (BR_CARD_W + BR_CARD_GAP)
                Local cy5 = gridY + row * (BR_CARD_H + BR_CARD_GAP)
                If cy5 + BR_CARD_H < sh - BR_BOT_RIBBON
                    Local h5 = Browser_DrawCardChrome("faction", i, cx5, cy5, mx, my, clicked)
                    Browser_DrawFactionCardBody(i, cx5, cy5)
                    If h5 Then clickedACard = True
                EndIf
                count = count + 1 : col = col + 1
                If col >= cols Then col = 0 : row = row + 1
            EndIf
        Next
    Else If Browser_Category$ = "animset"
        For As.AnimSet = Each AnimSet
            Local cx6 = gridX + col * (BR_CARD_W + BR_CARD_GAP)
            Local cy6 = gridY + row * (BR_CARD_H + BR_CARD_GAP)
            If cy6 + BR_CARD_H < sh - BR_BOT_RIBBON
                Local h6 = Browser_DrawCardChrome("animset", As\ID, cx6, cy6, mx, my, clicked)
                Browser_DrawAnimSetCardBody(As, cx6, cy6)
                If h6 Then clickedACard = True
            EndIf
            count = count + 1 : col = col + 1
            If col >= cols Then col = 0 : row = row + 1
        Next
    EndIf

    // Empty-state copy
    If count = 0
        LoomTextCentered(sw / 2, sh / 2, "No " + Browser_Category$ + "s in this project yet.", LOOM_STONE_200_R, LOOM_STONE_200_G, LOOM_STONE_200_B)
    EndIf

    Return clickedACard
End Function


// Draws the shared card chrome (background, hover border, kind eyebrow)
// and registers the rect for hit-testing. Returns True if this card was
// clicked this frame (and calls Threads_Focus as a side effect).
Function Browser_DrawCardChrome(kind$, refID, x, y, mx, my, clicked)
    Local hovered = (mx >= x And mx < x + BR_CARD_W And my >= y And my < y + BR_CARD_H)

    LoomFill(x, y, BR_CARD_W, BR_CARD_H, LOOM_STONE_800_R, LOOM_STONE_800_G, LOOM_STONE_800_B)

    If hovered = True
        LoomBorder(x, y, BR_CARD_W, BR_CARD_H, LOOM_ARCANE_500_R, LOOM_ARCANE_500_G, LOOM_ARCANE_500_B)
        LoomBorder(x + 1, y + 1, BR_CARD_W - 2, BR_CARD_H - 2, LOOM_ARCANE_500_R, LOOM_ARCANE_500_G, LOOM_ARCANE_500_B)
    Else
        LoomBorder(x, y, BR_CARD_W, BR_CARD_H, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
    EndIf

    // Top brass accent
    LoomHRule(x + 12, y + 8, BR_CARD_W - 24, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

    If hovered And clicked
        Threads_Focus(kind$, refID)
        WriteLog(LoomLog, "Browser: focused " + kind$ + "#" + Str(refID))
        Return True
    EndIf
    Return False
End Function


// -----------------------------------------------------------------------------
// Per-kind card body content
// -----------------------------------------------------------------------------

Function Browser_DrawActorCardBody(Ac.Actor, x, y)
    LoomText(x + 12, y + 18, Ac\Race$ + " [" + Ac\Class$ + "]", LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

    Local facName$ = FactionNames$(Ac\DefaultFaction)
    If facName$ = "" Then facName$ = "(no faction)"
    LoomText(x + 12, y + 44, "Faction", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomText(x + 12, y + 60, facName$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

    LoomText(x + 180, y + 44, "XP mult", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomText(x + 180, y + 60, Str(Ac\XPMultiplier), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
End Function


Function Browser_DrawItemCardBody(It.Item, x, y)
    LoomText(x + 12, y + 18, It\Name$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

    Local typeLabel$ = Browser_ItemTypeLabel$(It\ItemType)
    LoomText(x + 12, y + 44, "Type", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomText(x + 12, y + 60, typeLabel$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

    LoomText(x + 180, y + 44, "Value", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomText(x + 180, y + 60, Str(It\Value), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
End Function


Function Browser_DrawSpellCardBody(Sp.Spell, x, y)
    LoomText(x + 12, y + 18, Sp\Name$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

    LoomText(x + 12, y + 44, "Recharge", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomText(x + 12, y + 60, Str(Sp\RechargeTime / 1000) + " s", LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

    If Sp\Script$ <> ""
        LoomText(x + 180, y + 44, "Script", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomText(x + 180, y + 60, Sp\Script$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    EndIf
End Function


Function Browser_DrawZoneCardBody(Ar.Area, x, y)
    LoomText(x + 12, y + 18, Ar\Name$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

    Local portals = 0, spawns = 0, triggers = 0
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

    LoomText(x + 12,  y + 44, "Portals",  LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomText(x + 12,  y + 60, Str(portals), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    LoomText(x + 110, y + 44, "Spawns",   LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomText(x + 110, y + 60, Str(spawns), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    LoomText(x + 208, y + 44, "Triggers", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomText(x + 208, y + 60, Str(triggers), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
End Function


Function Browser_DrawFactionCardBody(idx, x, y)
    LoomText(x + 12, y + 18, FactionNames$(idx), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

    // Member count: walk actors with DefaultFaction == idx
    Local members = 0
    For Ac.Actor = Each Actor
        If Ac\DefaultFaction = idx Then members = members + 1
    Next

    LoomText(x + 12, y + 44, "Members", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomText(x + 12, y + 60, Str(members), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
End Function


Function Browser_DrawAnimSetCardBody(As.AnimSet, x, y)
    LoomText(x + 12, y + 18, As\Name$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

    // Count populated animation slots
    Local clips = 0
    Local i = 0
    For i = 0 To 149
        If As\AnimName$[i] <> "" Then clips = clips + 1
    Next

    // Count actors using this anim set (M or F)
    Local users = 0
    For Ac.Actor = Each Actor
        If Ac\MAnimationSet = As\ID Or Ac\FAnimationSet = As\ID Then users = users + 1
    Next

    LoomText(x + 12,  y + 44, "Clips", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomText(x + 12,  y + 60, Str(clips), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    LoomText(x + 110, y + 44, "Used by",  LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    LoomText(x + 110, y + 60, Str(users), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
End Function


// -----------------------------------------------------------------------------
// Footer
// -----------------------------------------------------------------------------
Function Browser_DrawFooter(sw, sh)
    Local y = sh - BR_BOT_RIBBON
    LoomFill(0, y, sw, BR_BOT_RIBBON, LOOM_STONE_850_R, LOOM_STONE_850_G, LOOM_STONE_850_B)
    LoomHRule(0, y, sw, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)

    LoomText(20, y + 10, "click a card to focus  ·  follow threads in the composer  ·  Esc to exit", LOOM_STONE_200_R, LOOM_STONE_200_G, LOOM_STONE_200_B)
End Function


// -----------------------------------------------------------------------------
// Helper: human-friendly item type label.
// rcce2 stores item types as ints (defined in Inventories.bb constants).
// -----------------------------------------------------------------------------
Function Browser_ItemTypeLabel$(t)
    If t = 0 Then Return "Other"
    If t = 1 Then Return "Weapon"
    If t = 2 Then Return "Armour"
    If t = 3 Then Return "Ring"
    If t = 4 Then Return "Potion"
    If t = 5 Then Return "Food"
    If t = 6 Then Return "Image"
    Return "Type " + Str(t)
End Function
