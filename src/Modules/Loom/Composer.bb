Strict

// =============================================================================
// Loom/Composer.bb -- per-kind detail page for the focused entity
// =============================================================================
//
// When the Browser focuses an entity (or a thread chip jumps), the Composer
// paints that entity's properties on the right side of the screen. Each kind
// has its own field layout. Reference fields render as thread chips via the
// held Threads instance; clicking a chip jumps and pushes the current focus
// onto the back stack.
//
// Reads:
//   self\threads\focusKind / focusID  (the Threads instance held at construction)
//   the underlying data modules' globals (ActorList, ItemList, SpellsList,
//   Each Area, FactionNames$, Each AnimSet)
//
// Writes:
//   Nothing. Read-only in the alpha.
//
// Architecture: Type with Methods, called as `Composer::method(self, args)`.


Const CMP_W           = 380
Const CMP_TOP         = 56     // matches BR_TOP_RIBBON
Const CMP_BOT_PAD     = 36     // matches BR_BOT_RIBBON
Const CMP_PAD         = 16
Const CMP_ROW_H       = 22
Const CMP_CHIP_H      = 26


// =============================================================================
// Composer -- right-side property panel.
// =============================================================================
Type Composer
    Field threads.Threads      // shared focus state, set by caller

    // Per-frame chip-click latch -- the per-kind body renderers set this when
    // any thread chip consumed a click, and renderAndUpdate returns it so the
    // caller can react (e.g. log it, refresh another surface).
    Field chipHit%


    Method create.Composer(threads.Threads)
        self\threads = threads
        self\chipHit = False
        Return self
    End Method


    // -------------------------------------------------------------------------
    // width -- 0 when nothing's focused (lets the Browser fill the screen
    // without leaving an empty right gutter), else CMP_W.
    // -------------------------------------------------------------------------
    Method width%()
        If self\threads\focusKind = "" Then Return 0
        Return CMP_W
    End Method


    // -------------------------------------------------------------------------
    // renderAndUpdate -- per-frame paint + chip hit-test. No-op when nothing
    // is focused. Returns True if any chip was clicked this frame.
    // -------------------------------------------------------------------------
    Method renderAndUpdate%(sw%, sh%)
        If self\threads\focusKind = "" Then Return False

        Local mx% = MouseX()
        Local my% = MouseY()
        Local clicked% = MouseHit(1)

        Local x% = sw - CMP_W
        Local y% = CMP_TOP
        Local w% = CMP_W
        Local h% = sh - CMP_TOP - CMP_BOT_PAD

        // Panel chrome -- brass left rule signals the primary surface.
        LoomFill(x, y, w, h, LOOM_STONE_850_R, LOOM_STONE_850_G, LOOM_STONE_850_B)
        LoomBorder(x, y, w, h, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        LoomFill(x, y, 3, h, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

        // Title block
        Local kind$ = self\threads\focusKind
        Local kindLabel$ = Composer::kindLabel(self, kind)
        Local entityName$ = Threads::lookupName(self\threads, kind, self\threads\focusID)
        If entityName = "" Then entityName = "(unknown)"

        LoomText(x + CMP_PAD, y + CMP_PAD, kindLabel, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomText(x + CMP_PAD, y + CMP_PAD + 16, entityName, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
        LoomHRule(x + CMP_PAD, y + CMP_PAD + 38, w - CMP_PAD * 2, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)

        // Body -- per-kind render dispatch
        Local bodyY% = y + CMP_PAD + 50
        Local bodyH% = h - (bodyY - y) - 24
        self\chipHit = False

        If kind = "actor"
            Composer::renderActor(self, x, bodyY, w, bodyH, mx, my, clicked)
        Else If kind = "item"
            Composer::renderItem(self, x, bodyY, w, bodyH, mx, my, clicked)
        Else If kind = "spell"
            Composer::renderSpell(self, x, bodyY, w, bodyH, mx, my, clicked)
        Else If kind = "zone"
            Composer::renderZone(self, x, bodyY, w, bodyH, mx, my, clicked)
        Else If kind = "faction"
            Composer::renderFaction(self, x, bodyY, w, bodyH, mx, my, clicked)
        Else If kind = "animset"
            Composer::renderAnimSet(self, x, bodyY, w, bodyH, mx, my, clicked)
        EndIf

        // Footer: back-stack hint
        Local stackSize% = ListSize(self\threads\backStack)
        Local footMsg$ = "Esc returns to browser"
        If stackSize > 0
            footMsg = "Esc walks back  ·  " + Str(stackSize) + " in trail"
        EndIf
        LoomText(x + CMP_PAD, y + h - 22, footMsg, LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)

        Return self\chipHit
    End Method


    // -------------------------------------------------------------------------
    // Layout helpers -- private-by-convention; called from per-kind renderers.
    // -------------------------------------------------------------------------

    // label : value row. Returns the next Y.
    Method row%(panelX%, panelW%, rowY%, label$, value$)
        LoomText(panelX + CMP_PAD,       rowY, label, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomText(panelX + CMP_PAD + 120, rowY, value, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
        Return rowY + CMP_ROW_H
    End Method


    // label : thread chip row. Returns the next Y. ORs into self\chipHit
    // when the chip is clicked so renderAndUpdate can surface it.
    Method chipRow%(panelX%, panelW%, rowY%, label$, kind$, refID%, mx%, my%, clicked%)
        LoomText(panelX + CMP_PAD, rowY + 4, label, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

        Local chipX% = panelX + CMP_PAD + 120
        Local chipW% = panelW - CMP_PAD * 2 - 120
        Local hit% = Threads::renderChip(self\threads, chipX, rowY, chipW, CMP_CHIP_H, kind, refID, mx, my, clicked)
        If hit Then self\chipHit = True

        Return rowY + CMP_CHIP_H + 4
    End Method


    // Section header -- brass rule + brass label. Returns the next Y.
    Method sectionHeader%(panelX%, panelW%, rowY%, title$)
        LoomHRule(panelX + CMP_PAD, rowY + 6, panelW - CMP_PAD * 2, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        LoomText(panelX + CMP_PAD, rowY + 10, title, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        Return rowY + 28
    End Method


    Method kindLabel$(kind$)
        If kind = "actor"   Then Return "ACTOR"
        If kind = "item"    Then Return "ITEM"
        If kind = "spell"   Then Return "SPELL"
        If kind = "zone"    Then Return "ZONE"
        If kind = "faction" Then Return "FACTION"
        If kind = "animset" Then Return "ANIMATION SET"
        Return Upper$(kind)
    End Method


    // -------------------------------------------------------------------------
    // Per-kind body renderers. Each lays out rows starting at bodyY and
    // returns void; chipHit is latched onto self for renderAndUpdate to see.
    // -------------------------------------------------------------------------

    Method renderActor(panelX%, bodyY%, panelW%, bodyH%, mx%, my%, clicked%)
        Local refID% = self\threads\focusID
        If refID < 0 Or refID > 65535 Then Return
        Local A.Actor = ActorList(refID)
        If A = Null Then Return

        Local y% = bodyY
        y = Composer::row(self, panelX, panelW, y, "ID",            Str(A\ID))
        y = Composer::row(self, panelX, panelW, y, "Race",          A\Race$)
        y = Composer::row(self, panelX, panelW, y, "Class",         A\Class$)
        y = Composer::row(self, panelX, panelW, y, "Aggressiveness", Composer::actorAggLabel(self, A\Aggressiveness))
        y = Composer::row(self, panelX, panelW, y, "Genders",       Composer::actorGenderLabel(self, A\Genders))
        y = Composer::row(self, panelX, panelW, y, "Playable",      Composer::boolLabel(self, A\Playable))
        y = Composer::row(self, panelX, panelW, y, "Rideable",      Composer::boolLabel(self, A\Rideable))
        y = Composer::row(self, panelX, panelW, y, "XP multiplier", Str(A\XPMultiplier))

        y = Composer::sectionHeader(self, panelX, panelW, y, "Threads")

        y = Composer::chipRow(self, panelX, panelW, y, "Faction",    "faction", A\DefaultFaction, mx, my, clicked)
        y = Composer::chipRow(self, panelX, panelW, y, "M anim set", "animset", A\MAnimationSet,  mx, my, clicked)
        y = Composer::chipRow(self, panelX, panelW, y, "F anim set", "animset", A\FAnimationSet,  mx, my, clicked)
    End Method


    Method renderItem(panelX%, bodyY%, panelW%, bodyH%, mx%, my%, clicked%)
        Local refID% = self\threads\focusID
        If refID < 0 Or refID > 65534 Then Return
        Local It.Item = ItemList(refID)
        If It = Null Then Return

        Local y% = bodyY
        y = Composer::row(self, panelX, panelW, y, "ID",        Str(It\ID))
        y = Composer::row(self, panelX, panelW, y, "Type",      Composer::itemTypeLabel(self, It\ItemType))
        y = Composer::row(self, panelX, panelW, y, "Slot",      Str(It\SlotType))
        y = Composer::row(self, panelX, panelW, y, "Value",     Str(It\Value))
        y = Composer::row(self, panelX, panelW, y, "Mass",      Str(It\Mass))
        y = Composer::row(self, panelX, panelW, y, "Stackable", Composer::boolLabel(self, It\Stackable))
        y = Composer::row(self, panelX, panelW, y, "Breakable", Composer::boolLabel(self, It\TakesDamage))

        // Weapon-specific
        If It\ItemType = 1
            y = Composer::sectionHeader(self, panelX, panelW, y, "Weapon")
            y = Composer::row(self, panelX, panelW, y, "Damage",      Str(It\WeaponDamage))
            y = Composer::row(self, panelX, panelW, y, "Weapon type", Str(It\WeaponType))
            If It\Range# > 0.0
                y = Composer::row(self, panelX, panelW, y, "Range",   Composer::formatFloat(self, It\Range#))
            EndIf
        EndIf

        // Armour-specific
        If It\ItemType = 2
            y = Composer::sectionHeader(self, panelX, panelW, y, "Armour")
            y = Composer::row(self, panelX, panelW, y, "Armour level", Str(It\ArmourLevel))
        EndIf

        // Restrictions
        If It\ExclusiveRace$ <> "" Or It\ExclusiveClass$ <> ""
            y = Composer::sectionHeader(self, panelX, panelW, y, "Restricted to")
            If It\ExclusiveRace$ <> ""
                y = Composer::row(self, panelX, panelW, y, "Race",  It\ExclusiveRace$)
            EndIf
            If It\ExclusiveClass$ <> ""
                y = Composer::row(self, panelX, panelW, y, "Class", It\ExclusiveClass$)
            EndIf
        EndIf

        // Script
        If It\Script$ <> ""
            y = Composer::sectionHeader(self, panelX, panelW, y, "Script")
            y = Composer::row(self, panelX, panelW, y, "Bound", It\Script$)
            If It\SMethod$ <> ""
                y = Composer::row(self, panelX, panelW, y, "Method", It\SMethod$)
            EndIf
        EndIf
    End Method


    Method renderSpell(panelX%, bodyY%, panelW%, bodyH%, mx%, my%, clicked%)
        Local refID% = self\threads\focusID
        If refID < 0 Or refID > 65534 Then Return
        Local S.Spell = SpellsList(refID)
        If S = Null Then Return

        Local y% = bodyY
        y = Composer::row(self, panelX, panelW, y, "ID",       Str(S\ID))
        y = Composer::row(self, panelX, panelW, y, "Recharge", Str(S\RechargeTime) + " ms")

        If S\Description$ <> ""
            y = Composer::sectionHeader(self, panelX, panelW, y, "Description")
            // Description can be long; clip to one line for now. Word-wrap is
            // a future enhancement.
            Local desc$ = S\Description$
            If Len(desc) > 60 Then desc = Left$(desc, 57) + "..."
            LoomText(panelX + CMP_PAD, y, desc, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
            y = y + CMP_ROW_H + 4
        EndIf

        If S\ExclusiveRace$ <> "" Or S\ExclusiveClass$ <> ""
            y = Composer::sectionHeader(self, panelX, panelW, y, "Restricted to")
            If S\ExclusiveRace$  <> "" Then y = Composer::row(self, panelX, panelW, y, "Race",  S\ExclusiveRace$)
            If S\ExclusiveClass$ <> "" Then y = Composer::row(self, panelX, panelW, y, "Class", S\ExclusiveClass$)
        EndIf

        If S\Script$ <> ""
            y = Composer::sectionHeader(self, panelX, panelW, y, "Script")
            y = Composer::row(self, panelX, panelW, y, "Bound", S\Script$)
            If S\SMethod$ <> "" Then y = Composer::row(self, panelX, panelW, y, "Method", S\SMethod$)
        EndIf
    End Method


    Method renderZone(panelX%, bodyY%, panelW%, bodyH%, mx%, my%, clicked%)
        Local Ar.Area = Object.Area(self\threads\focusID)
        If Ar = Null Then Return

        Local y% = bodyY
        y = Composer::row(self, panelX, panelW, y, "Name",     Ar\Name$)
        y = Composer::row(self, panelX, panelW, y, "Outdoors", Composer::boolLabel(self, Ar\Outdoors))
        y = Composer::row(self, panelX, panelW, y, "PvP",      Composer::boolLabel(self, Ar\PvP))
        y = Composer::row(self, panelX, panelW, y, "Gravity",  Str(Ar\Gravity))

        // Counts
        Local portals% = 0
        Local spawns% = 0
        Local triggers% = 0
        Local waypoints% = 0
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
        For i = 0 To 1999
            If Ar\WaypointX#[i] <> 0.0 Or Ar\WaypointZ#[i] <> 0.0 Then waypoints = waypoints + 1
        Next

        y = Composer::sectionHeader(self, panelX, panelW, y, "Contents")
        y = Composer::row(self, panelX, panelW, y, "Portals",   Str(portals))
        y = Composer::row(self, panelX, panelW, y, "Spawns",    Str(spawns))
        y = Composer::row(self, panelX, panelW, y, "Triggers",  Str(triggers))
        y = Composer::row(self, panelX, panelW, y, "Waypoints", Str(waypoints))

        // Scripts
        If Ar\EntryScript$ <> "" Or Ar\ExitScript$ <> ""
            y = Composer::sectionHeader(self, panelX, panelW, y, "Scripts")
            If Ar\EntryScript$ <> "" Then y = Composer::row(self, panelX, panelW, y, "Entry", Ar\EntryScript$)
            If Ar\ExitScript$  <> "" Then y = Composer::row(self, panelX, panelW, y, "Exit",  Ar\ExitScript$)
        EndIf

        // Portal links -- one chip per portal whose target resolves to a zone
        // we know about. The most-useful thread set zones can offer.
        If portals > 0
            y = Composer::sectionHeader(self, panelX, panelW, y, "Portal links")
            Local p% = 0
            For p = 0 To 99
                If Ar\PortalName$[p] <> "" And y < bodyY + bodyH - CMP_CHIP_H - 24
                    Local targetHandle% = Composer::findZoneByName(self, Ar\PortalLinkArea$[p])
                    If targetHandle <> 0
                        y = Composer::chipRow(self, panelX, panelW, y, Ar\PortalName$[p], "zone", targetHandle, mx, my, clicked)
                    Else
                        // Unknown target -- render a brass label that names it in danger-red.
                        LoomText(panelX + CMP_PAD, y + 4, Ar\PortalName$[p], LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
                        Local tgt$ = Ar\PortalLinkArea$[p]
                        If tgt = "" Then tgt = "(no target)"
                        LoomText(panelX + CMP_PAD + 120, y + 4, tgt, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
                        y = y + CMP_ROW_H
                    EndIf
                EndIf
            Next
        EndIf
    End Method


    Method renderFaction(panelX%, bodyY%, panelW%, bodyH%, mx%, my%, clicked%)
        Local idx% = self\threads\focusID
        If idx < 0 Or idx > 99 Then Return

        Local y% = bodyY
        y = Composer::row(self, panelX, panelW, y, "Name",  FactionNames$(idx))
        y = Composer::row(self, panelX, panelW, y, "Index", Str(idx))

        // Members -- every actor whose DefaultFaction matches. Each renders
        // as an actor chip. Capped to whatever fits in the panel body.
        y = Composer::sectionHeader(self, panelX, panelW, y, "Members")

        Local memberCount% = 0
        For Ac.Actor = Each Actor
            If Ac\DefaultFaction = idx And y < bodyY + bodyH - CMP_CHIP_H - 24
                y = Composer::chipRow(self, panelX, panelW, y, "", "actor", Ac\ID, mx, my, clicked)
                memberCount = memberCount + 1
            EndIf
        Next

        If memberCount = 0
            LoomText(panelX + CMP_PAD, y + 4, "(no members)", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)
        EndIf
    End Method


    Method renderAnimSet(panelX%, bodyY%, panelW%, bodyH%, mx%, my%, clicked%)
        Local targetID% = self\threads\focusID

        // AnimSet is iterated, not indexed -- walk to find.
        Local A.AnimSet = Null
        For As.AnimSet = Each AnimSet
            If As\ID = targetID Then A = As : Exit
        Next
        If A = Null Then Return

        Local y% = bodyY
        y = Composer::row(self, panelX, panelW, y, "Name", A\Name$)
        y = Composer::row(self, panelX, panelW, y, "ID",   Str(A\ID))

        Local clips% = 0
        Local i% = 0
        For i = 0 To 149
            If A\AnimName$[i] <> "" Then clips = clips + 1
        Next
        y = Composer::row(self, panelX, panelW, y, "Clips", Str(clips))

        y = Composer::sectionHeader(self, panelX, panelW, y, "Used by")
        Local userCount% = 0
        For Ac.Actor = Each Actor
            If (Ac\MAnimationSet = targetID Or Ac\FAnimationSet = targetID) And y < bodyY + bodyH - CMP_CHIP_H - 24
                y = Composer::chipRow(self, panelX, panelW, y, "", "actor", Ac\ID, mx, my, clicked)
                userCount = userCount + 1
            EndIf
        Next

        If userCount = 0
            LoomText(panelX + CMP_PAD, y + 4, "(no users)", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)
        EndIf
    End Method


    // -------------------------------------------------------------------------
    // Value formatters + helpers
    // -------------------------------------------------------------------------

    Method boolLabel$(b%)
        If b Then Return "Yes"
        Return "No"
    End Method


    Method formatFloat$(v#)
        Local rounded# = Float(Int(v# * 10.0)) / 10.0
        Return Str$(rounded)
    End Method


    Method actorAggLabel$(a%)
        If a = 0 Then Return "Passive"
        If a = 1 Then Return "Defensive"
        If a = 2 Then Return "Always attacks"
        If a = 3 Then Return "Non-combatant"
        Return Str(a)
    End Method


    Method actorGenderLabel$(g%)
        If g = 0 Then Return "Both"
        If g = 1 Then Return "Male only"
        If g = 2 Then Return "Female only"
        If g = 3 Then Return "No gender"
        Return Str(g)
    End Method


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


    // findZoneByName -- resolve a zone-name string (from a portal's
    // PortalLinkArea$) to its Handle, or 0 if not found.
    Method findZoneByName%(name$)
        If name = "" Then Return 0
        For Ar.Area = Each Area
            If Upper$(Ar\Name$) = Upper$(name) Then Return Handle(Ar)
        Next
        Return 0
    End Method
End Type
