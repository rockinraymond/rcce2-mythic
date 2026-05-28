Strict

// =============================================================================
// Loom/BrokenRefs.bb -- modal that enumerates every broken reference
// =============================================================================
//
// The Validation Conscience Ribbon (Ribbon.bb) shows a roll-up count of
// broken references. This modal expands that count into a one-row-per-
// broken-ref list with **click-to-jump** to the source entity so the user
// can actually fix the dangling reference.
//
// Activation: click the broken-ref count chip in the Ribbon (when count
// > 0). Closes on Esc or click-outside-modal, just like the other
// Loom modals (palette, timeline).
//
// What "broken" means here matches Ribbon's recomputeCache:
//   Actor . DefaultFaction        invalid index OR slot empty
//   Actor . MAnimationSet         non-zero ID that doesn't resolve
//   Actor . FAnimationSet         non-zero ID that doesn't resolve
//   Zone  . PortalLinkArea$[i]    non-empty name that doesn't resolve
//
// We re-scan on every open (the underlying data could have changed since
// the user last opened the modal); cheap at our scale and keeps the
// surface honest. Cap at BROKENREFS_MAX_ENTRIES so a fundamentally
// broken project doesn't render thousands of rows.
//
// Architecture: Type with Methods (modal owns its own keyboard pump +
// list scroll state). Holds a Threads reference for jump dispatch. The
// Composer ref isn't needed -- fixing the broken ref is a separate user
// action (jump to source -> edit field via the existing chip / picker
// flow).


Const BROKENREFS_MAX_ENTRIES = 250
Const BROKENREFS_MODAL_W     = 720
Const BROKENREFS_MODAL_H     = 480
Const BROKENREFS_PAD         = 16
Const BROKENREFS_ROW_H       = 24
Const BROKENREFS_HEADER_H    = 32
Const BROKENREFS_HINT_H      = 24


// -----------------------------------------------------------------------------
// BrokenRef -- one diagnostic. Allocated by rebuild, freed by clearList.
// -----------------------------------------------------------------------------
Type BrokenRef
    Field SourceKind$        // entity kind that holds the broken ref
    Field SourceRefID%       // entity ID
    Field SourceLabel$       // cached display name at scan time
    Field FieldDesc$         // field name + indexer if any (e.g. "portal[3]")
    Field BadValue$          // string representation of the bad reference
    Field Diagnosis$         // human-friendly explanation
End Type


// =============================================================================
// BrokenRefs -- enumerate-and-jump modal.
// =============================================================================
Type BrokenRefs
    Field threads.Threads

    Field open%
    Field entryCount%
    Field scrollOffset%


    Method create.BrokenRefs(threads.Threads)
        self\threads = threads
        self\open = False
        self\entryCount = 0
        self\scrollOffset = 0
        Return self
    End Method


    Method isOpen%()
        Return self\open
    End Method


    Method openModal()
        self\open = True
        self\scrollOffset = 0
        BrokenRefs::rebuild(self)
        FlushKeys
        WriteLog(LoomLog, "BrokenRefs: open (" + Str(self\entryCount) + " entries)")
    End Method


    Method closeModal()
        self\open = False
        BrokenRefs::clearList(self)
        WriteLog(LoomLog, "BrokenRefs: close")
    End Method


    Method clearList()
        Local r.BrokenRef
        For r = Each BrokenRef
            Delete r
        Next
        self\entryCount = 0
    End Method


    // -------------------------------------------------------------------------
    // rebuild -- scan every entity, emit one BrokenRef per dangling field.
    // Mirrors Ribbon::recomputeCache's checks; kept independent rather than
    // shared because each surface needs slightly different output (count vs
    // per-ref detail).
    //
    // Counters live on self\* because Strict rejects reassigning Method
    // Locals from inside nested For/If blocks.
    // -------------------------------------------------------------------------
    Method rebuild()
        BrokenRefs::clearList(self)

        // Actors
        For Ac.Actor = Each Actor
            If self\entryCount >= BROKENREFS_MAX_ENTRIES Then Exit
            BrokenRefs::scanActor(self, Ac)
        Next

        // Zones (portals)
        For Ar.Area = Each Area
            If self\entryCount >= BROKENREFS_MAX_ENTRIES Then Exit
            BrokenRefs::scanZone(self, Ar)
        Next

        // Clamp scroll
        If self\scrollOffset >= self\entryCount Then self\scrollOffset = self\entryCount - 1
        If self\scrollOffset < 0 Then self\scrollOffset = 0
    End Method


    Method scanActor(Ac.Actor)
        Local label$ = Ac\Race$ + " [" + Ac\Class$ + "]"

        // DefaultFaction
        If Ac\DefaultFaction < 0 Or Ac\DefaultFaction > 99
            BrokenRefs::emit(self, "actor", Ac\ID, label, "DefaultFaction", Str(Ac\DefaultFaction), "faction index out of range")
        Else If Ac\DefaultFaction > 0 And FactionNames$(Ac\DefaultFaction) = ""
            BrokenRefs::emit(self, "actor", Ac\ID, label, "DefaultFaction", Str(Ac\DefaultFaction), "faction slot is empty (deleted?)")
        EndIf

        // MAnimationSet
        If Ac\MAnimationSet <> 0
            If BrokenRefs::animSetExists(self, Ac\MAnimationSet) = False
                BrokenRefs::emit(self, "actor", Ac\ID, label, "MAnimationSet", Str(Ac\MAnimationSet), "anim set ID doesn't resolve")
            EndIf
        EndIf

        // FAnimationSet
        If Ac\FAnimationSet <> 0
            If BrokenRefs::animSetExists(self, Ac\FAnimationSet) = False
                BrokenRefs::emit(self, "actor", Ac\ID, label, "FAnimationSet", Str(Ac\FAnimationSet), "anim set ID doesn't resolve")
            EndIf
        EndIf
    End Method


    Method scanZone(Ar.Area)
        Local p% = 0
        For p = 0 To 99
            If Ar\PortalLinkArea$[p] <> ""
                If BrokenRefs::zoneExists(self, Ar\PortalLinkArea$[p]) = False
                    Local fieldDesc$ = "portal[" + Str(p) + "]"
                    If Ar\PortalName$[p] <> "" Then fieldDesc = fieldDesc + " (" + Ar\PortalName$[p] + ")"
                    BrokenRefs::emit(self, "zone", Handle(Ar), Ar\Name$, fieldDesc, Ar\PortalLinkArea$[p], "target zone name doesn't resolve")
                EndIf
            EndIf
        Next
    End Method


    Method emit(sourceKind$, sourceRefID%, sourceLabel$, fieldDesc$, badValue$, diagnosis$)
        Local r.BrokenRef = New BrokenRef()
        r\SourceKind = sourceKind
        r\SourceRefID = sourceRefID
        r\SourceLabel = sourceLabel
        r\FieldDesc = fieldDesc
        r\BadValue = badValue
        r\Diagnosis = diagnosis
        self\entryCount = self\entryCount + 1
    End Method


    Method animSetExists%(id%)
        Local As.AnimSet
        For As = Each AnimSet
            If As\ID = id Then Return True
        Next
        Return False
    End Method


    Method zoneExists%(name$)
        Local upr$ = Upper$(name)
        Local Ar.Area
        For Ar = Each Area
            If Upper$(Ar\Name$) = upr Then Return True
        Next
        Return False
    End Method


    // -------------------------------------------------------------------------
    // renderAndUpdate -- per-frame paint + input. Returns True when the
    // modal consumed input so the outer Loom frame skips its own Esc.
    // -------------------------------------------------------------------------
    Method renderAndUpdate%(sw%, sh%)
        If self\open = False Then Return False

        BrokenRefs::pumpKeyboard(self)
        If self\open = False Then Return True

        // Dim background, draw centered modal
        LoomFill(0, 0, sw, sh, LOOM_STONE_950_R, LOOM_STONE_950_G, LOOM_STONE_950_B)

        Local mx% = MouseX()
        Local my% = MouseY()
        Local clicked% = MouseHit(1)

        Local modalX% = (sw - BROKENREFS_MODAL_W) / 2
        Local modalY% = (sh - BROKENREFS_MODAL_H) / 3

        LoomFill(modalX, modalY, BROKENREFS_MODAL_W, BROKENREFS_MODAL_H, LOOM_STONE_850_R, LOOM_STONE_850_G, LOOM_STONE_850_B)
        LoomBorder(modalX, modalY, BROKENREFS_MODAL_W, BROKENREFS_MODAL_H, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
        LoomBorder(modalX + 1, modalY + 1, BROKENREFS_MODAL_W - 2, BROKENREFS_MODAL_H - 2, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        LoomFill(modalX, modalY, BROKENREFS_MODAL_W, 3, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)

        // Header
        Local headerTxt$ = "BROKEN REFERENCES  |  " + Str(self\entryCount)
        If self\entryCount >= BROKENREFS_MAX_ENTRIES Then headerTxt = headerTxt + "+ (capped)"
        LoomText(modalX + BROKENREFS_PAD, modalY + 10, headerTxt, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)

        BrokenRefs::drawEntries(self, modalX, modalY + BROKENREFS_HEADER_H, mx, my, clicked)

        // Footer hint
        Local hy% = modalY + BROKENREFS_MODAL_H - BROKENREFS_HINT_H - 4
        LoomHRule(modalX + BROKENREFS_PAD, hy - 2, BROKENREFS_MODAL_W - BROKENREFS_PAD * 2, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        LoomText(modalX + BROKENREFS_PAD, hy + 4, "Click a row to jump to the source  |  arrows scroll  |  Esc to close", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)

        If clicked = True
            If mx < modalX Or mx >= modalX + BROKENREFS_MODAL_W Or my < modalY Or my >= modalY + BROKENREFS_MODAL_H
                BrokenRefs::closeModal(self)
            EndIf
        EndIf

        Return True
    End Method


    Method drawEntries(modalX%, listY%, mx%, my%, clicked%)
        Local listH% = BROKENREFS_MODAL_H - BROKENREFS_HEADER_H - BROKENREFS_HINT_H - 12
        Local rowsVisible% = listH / BROKENREFS_ROW_H
        Local rx% = modalX + BROKENREFS_PAD
        Local rw% = BROKENREFS_MODAL_W - BROKENREFS_PAD * 2

        If self\entryCount = 0
            LoomText(rx, listY + 12, "No broken references. Project is clean.", LOOM_SUCCESS_R, LOOM_SUCCESS_G, LOOM_SUCCESS_B)
            Return
        EndIf

        Local skipped% = 0
        Local shown% = 0
        Local r.BrokenRef
        For r = Each BrokenRef
            If skipped < self\scrollOffset
                skipped = skipped + 1
            Else
                If shown >= rowsVisible Then Exit
                Local ry% = listY + shown * BROKENREFS_ROW_H
                BrokenRefs::drawOneEntry(self, r, rx, ry, rw, mx, my, clicked)
                shown = shown + 1
            EndIf
        Next
    End Method


    Method drawOneEntry(r.BrokenRef, rx%, ry%, rw%, mx%, my%, clicked%)
        Local hovered% = (mx >= rx And mx < rx + rw And my >= ry And my < ry + BROKENREFS_ROW_H)
        If hovered = True
            LoomFill(rx, ry, rw, BROKENREFS_ROW_H, LOOM_STONE_700_R, LOOM_STONE_700_G, LOOM_STONE_700_B)
        EndIf

        // Source: kind + label
        LoomText(rx + 6, ry + 4, r\SourceKind + "  " + r\SourceLabel, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        // Field + bad value
        Local mid$ = r\FieldDesc + " = " + Chr(34) + BrokenRefs::truncate(self, r\BadValue, 20) + Chr(34)
        LoomText(rx + 260, ry + 4, mid, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)

        // Diagnosis on the right
        LoomText(rx + rw - StringWidth(r\Diagnosis) - 12, ry + 4, r\Diagnosis, LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)

        If hovered And clicked
            BrokenRefs::closeModal(self)
            Threads::focus(self\threads, r\SourceKind, r\SourceRefID)
            WriteLog(LoomLog, "BrokenRefs: jumped to " + r\SourceKind + "#" + Str(r\SourceRefID) + " for " + r\FieldDesc)
        EndIf
    End Method


    Method pumpKeyboard()
        If KeyHit(1)
            BrokenRefs::closeModal(self)
            Return
        EndIf
        If KeyHit(200) And self\scrollOffset > 0
            self\scrollOffset = self\scrollOffset - 1
        EndIf
        If KeyHit(208)
            self\scrollOffset = self\scrollOffset + 1
            If self\scrollOffset >= self\entryCount Then self\scrollOffset = self\entryCount - 1
            If self\scrollOffset < 0 Then self\scrollOffset = 0
        EndIf
    End Method


    Method truncate$(s$, maxLen%)
        If Len(s) <= maxLen Then Return s
        Return Left$(s, maxLen - 2) + ".."
    End Method
End Type
