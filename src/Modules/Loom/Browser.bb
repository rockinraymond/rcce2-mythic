Strict

// =============================================================================
// Loom/Browser.bb -- everything-browser (entity picker grid by category)
// =============================================================================
//
// The boot surface. Six categories (actor / item / spell / zone / faction /
// animset), each renders as a grid of clickable cards. Clicking a card calls
// Threads::focus on the held Threads reference, which the Composer then reads
// to paint its detail page.
//
// Per-category card content (kept compact so 3 cards fit per row at 1280):
//   actor   : "Race [Class]" + faction name + XP multiplier
//   item    : name + type label (Weapon / Armour / Potion / etc.) + value
//   spell   : name + "Recharge Ns" subtitle
//   zone    : name + portal/spawn/trigger counts
//   faction : name + member count (computed: actors whose DefaultFaction
//             equals this faction index)
//   animset : name + clip count + computed "used by" count
//
// Architecture: Type with Methods, called as `Browser::method(self, args)`.
// Holds a reference to the shared Threads instance (set at construction)
// so card clicks can dispatch the focus change without globals.


// Layout constants
Const BR_TOP_RIBBON  = 56
Const BR_TAB_BAR_H   = 36
Const BR_BOT_RIBBON  = 36
Const BR_SECTION_PAD = 28
Const BR_CARD_W      = 300
Const BR_CARD_H      = 96
Const BR_CARD_GAP    = 14


// -----------------------------------------------------------------------------
// BrowserCategory -- one entry per category, iterated in insertion order to
// drive the tab bar's rendering order. Owned by Browser; instances are
// allocated in Browser::create and live for the Browser's lifetime.
// -----------------------------------------------------------------------------
Type BrowserCategory
    Field Kind$
    Field Title$
End Type


// =============================================================================
// Browser -- everything-grid surface.
// =============================================================================
Type Browser
    Field category$            // currently-selected kind: actor/item/spell/...
    Field threads.Threads      // shared focus state, set by caller

    // Per-frame click latch -- set inside drawCardChrome when a card is
    // clicked, read by drawCardGrid at the end of the frame. Lives on the
    // Type rather than as a Method-local because BlitzForge Strict mode
    // rejects re-assigning a Method-scope Local from inside nested If/For
    // blocks ("assignment should start with local/global/const"); Field
    // writes through `self\` work at any nesting depth.
    Field cardClickLatch%


    Method create.Browser(threads.Threads)
        self\threads = threads
        self\category = "actor"     // richest content; most useful starting point
        self\cardClickLatch = False

        // Build the ordered category list. Iterated via `Each BrowserCategory`
        // in insertion order (Blitz3D's global type pool is FIFO) -- also the
        // tab-bar order.
        Browser::addCategory(self, "actor",   "Actors")
        Browser::addCategory(self, "item",    "Items")
        Browser::addCategory(self, "spell",   "Spells")
        Browser::addCategory(self, "zone",    "Zones")
        Browser::addCategory(self, "faction", "Factions")
        Browser::addCategory(self, "animset", "Animation Sets")

        Return self
    End Method


    Method addCategory(kind$, title$)
        Local c.BrowserCategory = New BrowserCategory()
        c\Kind = kind$
        c\Title = title$
    End Method


    // -------------------------------------------------------------------------
    // renderAndUpdate -- per-frame paint + hit-test.
    // -------------------------------------------------------------------------
    Method renderAndUpdate%(sw%, sh%, project$)
        Local mx% = MouseX()
        Local my% = MouseY()
        Local clicked% = MouseHit(1)

        // Background gradient
        LoomGradientV(0, 0, sw, sh, LOOM_STONE_900_R, LOOM_STONE_900_G, LOOM_STONE_900_B, LOOM_STONE_950_R, LOOM_STONE_950_G, LOOM_STONE_950_B)

        // Chrome
        Browser::drawTopRibbon(self, sw, project$)
        Browser::drawTabBar(self, sw, mx, my, clicked)
        Browser::drawFooter(self, sw, sh)

        // Card grid
        Browser::drawCardGrid(self, sw, sh, mx, my, clicked)
        Return self\cardClickLatch
    End Method


    // -------------------------------------------------------------------------
    // Top brand strip
    // -------------------------------------------------------------------------
    Method drawTopRibbon(sw%, project$)
        LoomFill(0, 0, sw, BR_TOP_RIBBON, LOOM_STONE_850_R, LOOM_STONE_850_G, LOOM_STONE_850_B)
        LoomHRule(0, BR_TOP_RIBBON - 1, sw, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        LoomHRule(0, BR_TOP_RIBBON,     sw, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomHRule(0, BR_TOP_RIBBON + 1, sw, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)

        LoomText(20, 18, "LOOM",    LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
        LoomText(20, 32, "Browser", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

        LoomTextCentered(sw / 2, 22, project$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    End Method


    // -------------------------------------------------------------------------
    // Category tab bar -- active tab gets a brass underline. Hit-test inline.
    // -------------------------------------------------------------------------
    Method drawTabBar(sw%, mx%, my%, clicked%)
        Local y% = BR_TOP_RIBBON
        Local h% = BR_TAB_BAR_H
        LoomFill(0, y, sw, h, LOOM_STONE_800_R, LOOM_STONE_800_G, LOOM_STONE_800_B)
        LoomHRule(0, y + h, sw, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)

        Local x% = 20
        For c.BrowserCategory = Each BrowserCategory
            Local w% = StringWidth(c\Title) + 40
            Local active% = (c\Kind = self\category)
            Local hovered% = (mx >= x And mx < x + w And my >= y And my < y + h)

            If hovered = True
                LoomFill(x, y, w, h, LOOM_STONE_700_R, LOOM_STONE_700_G, LOOM_STONE_700_B)
            EndIf

            If active = True
                LoomText(x + 20, y + 11, c\Title, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
                LoomFill(x + 8, y + h - 3, w - 16, 3, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
            Else
                LoomText(x + 20, y + 11, c\Title, LOOM_STONE_200_R, LOOM_STONE_200_G, LOOM_STONE_200_B)
            EndIf

            If hovered And clicked
                self\category = c\Kind
                WriteLog(LoomLog, "Browser: category -> " + c\Kind)
            EndIf

            x = x + w + 6
        Next
    End Method


    // -------------------------------------------------------------------------
    // Card grid -- dispatcher to a per-kind grid method. Each per-kind method
    // owns its own loop counters and `count`; this avoids one giant
    // Else-If-chain Method that triggers Strict's nested-block reassignment
    // pathology, and is cleaner OO design anyway. Returns True via
    // self\cardClickLatch (which the per-kind methods set as a side effect).
    // -------------------------------------------------------------------------
    Method drawCardGrid%(sw%, sh%, mx%, my%, clicked%)
        Local gridX% = BR_SECTION_PAD
        Local gridY% = BR_TOP_RIBBON + BR_TAB_BAR_H + BR_SECTION_PAD
        Local gridW% = sw - (BR_SECTION_PAD * 2)
        Local cols% = (gridW + BR_CARD_GAP) / (BR_CARD_W + BR_CARD_GAP)
        If cols < 1 Then cols = 1

        self\cardClickLatch = False

        Local count% = 0
        Local cat$ = self\category

        If cat = "actor"
            count = Browser::drawActorGrid(self, sw, sh, mx, my, clicked, gridX, gridY, cols)
        EndIf
        If cat = "item"
            count = Browser::drawItemGrid(self, sw, sh, mx, my, clicked, gridX, gridY, cols)
        EndIf
        If cat = "spell"
            count = Browser::drawSpellGrid(self, sw, sh, mx, my, clicked, gridX, gridY, cols)
        EndIf
        If cat = "zone"
            count = Browser::drawZoneGrid(self, sw, sh, mx, my, clicked, gridX, gridY, cols)
        EndIf
        If cat = "faction"
            count = Browser::drawFactionGrid(self, sw, sh, mx, my, clicked, gridX, gridY, cols)
        EndIf
        If cat = "animset"
            count = Browser::drawAnimSetGrid(self, sw, sh, mx, my, clicked, gridX, gridY, cols)
        EndIf

        // Empty-state copy
        If count = 0
            LoomTextCentered(sw / 2, sh / 2, "No " + cat + "s in this project yet.", LOOM_STONE_200_R, LOOM_STONE_200_G, LOOM_STONE_200_B)
        EndIf

        Return self\cardClickLatch
    End Method


    // -------------------------------------------------------------------------
    // Per-kind grid renderers. Each iterates its data store, lays out cards,
    // sets self\cardClickLatch on hover-click, and returns the count of
    // cards rendered (for the dispatcher's empty-state check).
    // -------------------------------------------------------------------------

    Method drawActorGrid%(sw%, sh%, mx%, my%, clicked%, gridX%, gridY%, cols%)
        Local col% = 0
        Local row% = 0
        Local count% = 0
        For Ac.Actor = Each Actor
            Local cx% = gridX + col * (BR_CARD_W + BR_CARD_GAP)
            Local cy% = gridY + row * (BR_CARD_H + BR_CARD_GAP)
            If cy + BR_CARD_H < sh - BR_BOT_RIBBON
                Browser::drawCardChrome(self, "actor", Ac\ID, cx, cy, mx, my, clicked)
                Browser::drawActorCardBody(self, Ac, cx, cy)
            EndIf
            count = count + 1
            col = col + 1
            If col >= cols Then col = 0 : row = row + 1
        Next
        Return count
    End Method


    Method drawItemGrid%(sw%, sh%, mx%, my%, clicked%, gridX%, gridY%, cols%)
        Local col% = 0
        Local row% = 0
        Local count% = 0
        For It.Item = Each Item
            Local cx% = gridX + col * (BR_CARD_W + BR_CARD_GAP)
            Local cy% = gridY + row * (BR_CARD_H + BR_CARD_GAP)
            If cy + BR_CARD_H < sh - BR_BOT_RIBBON
                Browser::drawCardChrome(self, "item", It\ID, cx, cy, mx, my, clicked)
                Browser::drawItemCardBody(self, It, cx, cy)
            EndIf
            count = count + 1
            col = col + 1
            If col >= cols Then col = 0 : row = row + 1
        Next
        Return count
    End Method


    Method drawSpellGrid%(sw%, sh%, mx%, my%, clicked%, gridX%, gridY%, cols%)
        Local col% = 0
        Local row% = 0
        Local count% = 0
        For Sp.Spell = Each Spell
            Local cx% = gridX + col * (BR_CARD_W + BR_CARD_GAP)
            Local cy% = gridY + row * (BR_CARD_H + BR_CARD_GAP)
            If cy + BR_CARD_H < sh - BR_BOT_RIBBON
                Browser::drawCardChrome(self, "spell", Sp\ID, cx, cy, mx, my, clicked)
                Browser::drawSpellCardBody(self, Sp, cx, cy)
            EndIf
            count = count + 1
            col = col + 1
            If col >= cols Then col = 0 : row = row + 1
        Next
        Return count
    End Method


    Method drawZoneGrid%(sw%, sh%, mx%, my%, clicked%, gridX%, gridY%, cols%)
        Local col% = 0
        Local row% = 0
        Local count% = 0
        For Ar.Area = Each Area
            Local cx% = gridX + col * (BR_CARD_W + BR_CARD_GAP)
            Local cy% = gridY + row * (BR_CARD_H + BR_CARD_GAP)
            If cy + BR_CARD_H < sh - BR_BOT_RIBBON
                Browser::drawCardChrome(self, "zone", Handle(Ar), cx, cy, mx, my, clicked)
                Browser::drawZoneCardBody(self, Ar, cx, cy)
            EndIf
            count = count + 1
            col = col + 1
            If col >= cols Then col = 0 : row = row + 1
        Next
        Return count
    End Method


    Method drawFactionGrid%(sw%, sh%, mx%, my%, clicked%, gridX%, gridY%, cols%)
        Local col% = 0
        Local row% = 0
        Local count% = 0
        Local i% = 0
        For i = 0 To 99
            If FactionNames$(i) <> ""
                Local cx% = gridX + col * (BR_CARD_W + BR_CARD_GAP)
                Local cy% = gridY + row * (BR_CARD_H + BR_CARD_GAP)
                If cy + BR_CARD_H < sh - BR_BOT_RIBBON
                    Browser::drawCardChrome(self, "faction", i, cx, cy, mx, my, clicked)
                    Browser::drawFactionCardBody(self, i, cx, cy)
                EndIf
                count = count + 1
                col = col + 1
                If col >= cols Then col = 0 : row = row + 1
            EndIf
        Next
        Return count
    End Method


    Method drawAnimSetGrid%(sw%, sh%, mx%, my%, clicked%, gridX%, gridY%, cols%)
        Local col% = 0
        Local row% = 0
        Local count% = 0
        For As.AnimSet = Each AnimSet
            Local cx% = gridX + col * (BR_CARD_W + BR_CARD_GAP)
            Local cy% = gridY + row * (BR_CARD_H + BR_CARD_GAP)
            If cy + BR_CARD_H < sh - BR_BOT_RIBBON
                Browser::drawCardChrome(self, "animset", As\ID, cx, cy, mx, my, clicked)
                Browser::drawAnimSetCardBody(self, As, cx, cy)
            EndIf
            count = count + 1
            col = col + 1
            If col >= cols Then col = 0 : row = row + 1
        Next
        Return count
    End Method


    // -------------------------------------------------------------------------
    // drawCardChrome -- shared card background + hover border + brass accent +
    // inline hit-test. Sets self\cardClickLatch and calls Threads::focus on
    // click (side effects; no return value so per-kind grid methods don't
    // need to propagate booleans through nested scopes).
    // -------------------------------------------------------------------------
    Method drawCardChrome(kind$, refID%, x%, y%, mx%, my%, clicked%)
        Local hovered% = (mx >= x And mx < x + BR_CARD_W And my >= y And my < y + BR_CARD_H)

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
            Threads::focus(self\threads, kind, refID)
            self\cardClickLatch = True
            WriteLog(LoomLog, "Browser: focused " + kind + "#" + Str(refID))
        EndIf
    End Method


    // -------------------------------------------------------------------------
    // Per-kind card body content
    // -------------------------------------------------------------------------

    Method drawActorCardBody(Ac.Actor, x%, y%)
        LoomText(x + 12, y + 18, Ac\Race$ + " [" + Ac\Class$ + "]", LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        Local facName$ = FactionNames$(Ac\DefaultFaction)
        If facName = "" Then facName = "(no faction)"
        LoomText(x + 12, y + 44, "Faction", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomText(x + 12, y + 60, facName, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        LoomText(x + 180, y + 44, "XP mult", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomText(x + 180, y + 60, Str(Ac\XPMultiplier), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    End Method


    Method drawItemCardBody(It.Item, x%, y%)
        LoomText(x + 12, y + 18, It\Name$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        Local typeLabel$ = Browser::itemTypeLabel(self, It\ItemType)
        LoomText(x + 12, y + 44, "Type", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomText(x + 12, y + 60, typeLabel, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        LoomText(x + 180, y + 44, "Value", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomText(x + 180, y + 60, Str(It\Value), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    End Method


    Method drawSpellCardBody(Sp.Spell, x%, y%)
        LoomText(x + 12, y + 18, Sp\Name$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        LoomText(x + 12, y + 44, "Recharge", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomText(x + 12, y + 60, Str(Sp\RechargeTime / 1000) + " s", LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        If Sp\Script$ <> ""
            LoomText(x + 180, y + 44, "Script", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
            LoomText(x + 180, y + 60, Sp\Script$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
        EndIf
    End Method


    Method drawZoneCardBody(Ar.Area, x%, y%)
        LoomText(x + 12, y + 18, Ar\Name$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        Local portals% = 0
        Local spawns% = 0
        Local triggers% = 0
        Local i% = 0
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
    End Method


    Method drawFactionCardBody(idx%, x%, y%)
        LoomText(x + 12, y + 18, FactionNames$(idx), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        // Member count: walk actors with DefaultFaction == idx
        Local members% = 0
        For Ac.Actor = Each Actor
            If Ac\DefaultFaction = idx Then members = members + 1
        Next

        LoomText(x + 12, y + 44, "Members", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomText(x + 12, y + 60, Str(members), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    End Method


    Method drawAnimSetCardBody(As.AnimSet, x%, y%)
        LoomText(x + 12, y + 18, As\Name$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        Local clips% = 0
        Local i% = 0
        For i = 0 To 149
            If As\AnimName$[i] <> "" Then clips = clips + 1
        Next

        // Count actors using this anim set (M or F)
        Local users% = 0
        For Ac.Actor = Each Actor
            If Ac\MAnimationSet = As\ID Or Ac\FAnimationSet = As\ID Then users = users + 1
        Next

        LoomText(x + 12,  y + 44, "Clips", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomText(x + 12,  y + 60, Str(clips), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
        LoomText(x + 110, y + 44, "Used by",  LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomText(x + 110, y + 60, Str(users), LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    End Method


    // -------------------------------------------------------------------------
    // Footer
    // -------------------------------------------------------------------------
    Method drawFooter(sw%, sh%)
        Local y% = sh - BR_BOT_RIBBON
        LoomFill(0, y, sw, BR_BOT_RIBBON, LOOM_STONE_850_R, LOOM_STONE_850_G, LOOM_STONE_850_B)
        LoomHRule(0, y, sw, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)

        LoomText(20, y + 10, "click a card to focus  ·  follow threads in the composer  ·  Esc to exit", LOOM_STONE_200_R, LOOM_STONE_200_G, LOOM_STONE_200_B)
    End Method


    // -------------------------------------------------------------------------
    // itemTypeLabel -- human-friendly item type. rcce2 stores item types as
    // ints (defined in Inventories.bb constants).
    // -------------------------------------------------------------------------
    Method itemTypeLabel$(t%)
        If t = 0 Then Return "Other"
        If t = 1 Then Return "Weapon"
        If t = 2 Then Return "Armour"
        If t = 3 Then Return "Ring"
        If t = 4 Then Return "Potion"
        If t = 5 Then Return "Food"
        If t = 6 Then Return "Image"
        Return "Type " + Str(t)
    End Method
End Type
