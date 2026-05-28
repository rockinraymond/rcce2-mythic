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
    Field Severity$          // "error" / "warning" / "info" -- drives left-rail color
    Field Category$          // "broken-ref" / "empty-field" / "playability" /
                             // "weapon-config" / "spell-config" / "orphan-zone"
                             // Used for grouping in the modal.
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

        // Actors -- broken refs (factions, anim sets) + content checks +
        // missing-mesh (base body meshes only -- iterating all appearance
        // arrays would dwarf the modal).
        For Ac.Actor = Each Actor
            If self\entryCount >= BROKENREFS_MAX_ENTRIES Then Exit
            BrokenRefs::scanActor(self, Ac)
            BrokenRefs::scanActorContent(self, Ac)
            BrokenRefs::scanActorAssets(self, Ac)
        Next

        // Items -- empty name, weapon w/ 0 damage, asset checks (thumb +
        // M/F mesh)
        Local It.Item
        For ai% = 0 To 65534
            If self\entryCount >= BROKENREFS_MAX_ENTRIES Then Exit
            It = ItemList(ai)
            If It <> Null
                BrokenRefs::scanItemContent(self, It)
                BrokenRefs::scanItemAssets(self, It)
            EndIf
        Next

        // Spells -- empty name, missing script + thumbnail asset check
        Local Sp.Spell
        For asi% = 0 To 65534
            If self\entryCount >= BROKENREFS_MAX_ENTRIES Then Exit
            Sp = SpellsList(asi)
            If Sp <> Null
                BrokenRefs::scanSpellContent(self, Sp)
                BrokenRefs::scanSpellAssets(self, Sp)
            EndIf
        Next

        // Zones -- broken portal refs + orphan zone checks
        For Ar.Area = Each Area
            If self\entryCount >= BROKENREFS_MAX_ENTRIES Then Exit
            BrokenRefs::scanZone(self, Ar)
            BrokenRefs::scanZoneContent(self, Ar)
        Next

        // Clamp scroll
        If self\scrollOffset >= self\entryCount Then self\scrollOffset = self\entryCount - 1
        If self\scrollOffset < 0 Then self\scrollOffset = 0
    End Method


    // -------------------------------------------------------------------------
    // scanActorAssets -- check that the actor's primary mesh references
    // resolve to files on disk. Only checks MeshIDs[0] + MeshIDs[1]
    // (base male + female bodies); checking every gubbin + hair/beard/
    // face/body would balloon the modal for projects with intentional
    // empty slots.
    // -------------------------------------------------------------------------
    Method scanActorAssets(Ac.Actor)
        Local label$ = Ac\Race$ + " [" + Ac\Class$ + "]"
        If Ac\MeshIDs[0] > 0 And BrokenRefs::meshFileExists(self, Ac\MeshIDs[0]) = False
            BrokenRefs::emitFull(self, "actor", Ac\ID, label, "MeshIDs[0] (male base)", Str(Ac\MeshIDs[0]), "mesh ID has no file on disk", "warning", "missing-mesh")
        EndIf
        If Ac\MeshIDs[1] > 0 And BrokenRefs::meshFileExists(self, Ac\MeshIDs[1]) = False
            BrokenRefs::emitFull(self, "actor", Ac\ID, label, "MeshIDs[1] (female base)", Str(Ac\MeshIDs[1]), "mesh ID has no file on disk", "warning", "missing-mesh")
        EndIf
        If Ac\BloodTexID > 0 And BrokenRefs::textureFileExists(self, Ac\BloodTexID) = False
            BrokenRefs::emitFull(self, "actor", Ac\ID, label, "BloodTexID", Str(Ac\BloodTexID), "texture ID has no file on disk", "warning", "missing-texture")
        EndIf
    End Method


    // -------------------------------------------------------------------------
    // scanItemAssets -- check Item's headline visual refs.
    // -------------------------------------------------------------------------
    Method scanItemAssets(It.Item)
        If It\ThumbnailTexID > 0 And BrokenRefs::textureFileExists(self, It\ThumbnailTexID) = False
            BrokenRefs::emitFull(self, "item", It\ID, It\Name$, "ThumbnailTexID", Str(It\ThumbnailTexID), "texture ID has no file on disk", "warning", "missing-texture")
        EndIf
        If It\MMeshID > 0 And BrokenRefs::meshFileExists(self, It\MMeshID) = False
            BrokenRefs::emitFull(self, "item", It\ID, It\Name$, "MMeshID", Str(It\MMeshID), "mesh ID has no file on disk", "warning", "missing-mesh")
        EndIf
        If It\FMeshID > 0 And BrokenRefs::meshFileExists(self, It\FMeshID) = False
            BrokenRefs::emitFull(self, "item", It\ID, It\Name$, "FMeshID", Str(It\FMeshID), "mesh ID has no file on disk", "warning", "missing-mesh")
        EndIf
    End Method


    // -------------------------------------------------------------------------
    // scanSpellAssets -- check Spell's thumbnail.
    // -------------------------------------------------------------------------
    Method scanSpellAssets(Sp.Spell)
        If Sp\ThumbnailTexID > 0 And BrokenRefs::textureFileExists(self, Sp\ThumbnailTexID) = False
            BrokenRefs::emitFull(self, "spell", Sp\ID, Sp\Name$, "ThumbnailTexID", Str(Sp\ThumbnailTexID), "texture ID has no file on disk", "warning", "missing-texture")
        EndIf
    End Method


    // -------------------------------------------------------------------------
    // textureFileExists -- True if GetTextureName$(ID) resolves to a real
    // file under Data\Textures\. The cached lookup name has a trailing
    // flag byte (per Media.bb's GetTexture); strip it.
    // -------------------------------------------------------------------------
    Method textureFileExists%(ID%)
        Local NameAndFlags$ = GetTextureName$(ID)
        If NameAndFlags = "" Then Return False
        Local NameLen% = Len(NameAndFlags) - 1
        If NameLen < 1 Then Return False
        Local Name$ = Left$(NameAndFlags, NameLen)
        If Name = "" Then Return False
        If FileType("Data\Textures\" + Name) = 1 Then Return True
        Return False
    End Method


    // -------------------------------------------------------------------------
    // meshFileExists -- True if GetMeshNameClean$(ID) resolves to a file
    // under Data\Meshes\.
    // -------------------------------------------------------------------------
    Method meshFileExists%(ID%)
        Local Name$ = GetMeshNameClean$(ID)
        If Name = "" Then Return False
        If FileType("Data\Meshes\" + Name) = 1 Then Return True
        Return False
    End Method


    // -------------------------------------------------------------------------
    // scanActorContent -- non-reference checks. Severity warning (not error)
    // since these don't crash the engine but degrade gameplay.
    // -------------------------------------------------------------------------
    Method scanActorContent(Ac.Actor)
        Local label$ = Ac\Race$ + " [" + Ac\Class$ + "]"

        // Playable actor but no animation set -- player char will T-pose.
        If Ac\Playable = True
            If Ac\MAnimationSet = 0 And Ac\Genders <> 2
                BrokenRefs::emitFull(self, "actor", Ac\ID, label, "MAnimationSet", "0", "playable male has no anim set", "warning", "playability")
            EndIf
            If Ac\FAnimationSet = 0 And Ac\Genders <> 1
                BrokenRefs::emitFull(self, "actor", Ac\ID, label, "FAnimationSet", "0", "playable female has no anim set", "warning", "playability")
            EndIf
        EndIf

        // Race + Class both empty -- unidentifiable actor template
        If Ac\Race$ = "" And Ac\Class$ = ""
            BrokenRefs::emitFull(self, "actor", Ac\ID, "(unnamed)", "Race+Class", "(both empty)", "actor has no Race or Class identifier", "warning", "empty-field")
        EndIf
    End Method


    // -------------------------------------------------------------------------
    // scanItemContent -- content rules for Item templates.
    // -------------------------------------------------------------------------
    Method scanItemContent(It.Item)
        // Empty name -- item won't be findable in palette / search
        If It\Name$ = ""
            BrokenRefs::emitFull(self, "item", It\ID, "(unnamed)", "Name", "(empty)", "item has no Name", "warning", "empty-field")
        EndIf

        // Weapon (ItemType=1) with 0 WeaponDamage -- weapon is decorative
        If It\ItemType = 1 And It\WeaponDamage = 0
            BrokenRefs::emitFull(self, "item", It\ID, It\Name$, "WeaponDamage", "0", "weapon does no damage", "warning", "weapon-config")
        EndIf

        // Item has a script reference (Bound name) but no method -- nothing
        // will fire on use. Only flag when Script is set.
        If It\Script$ <> "" And It\SMethod$ = ""
            BrokenRefs::emitFull(self, "item", It\ID, It\Name$, "SMethod", "(empty)", "item has Bound script but no Method to call", "warning", "spell-config")
        EndIf
    End Method


    // -------------------------------------------------------------------------
    // scanSpellContent -- content rules for Spell templates.
    // -------------------------------------------------------------------------
    Method scanSpellContent(Sp.Spell)
        If Sp\Name$ = ""
            BrokenRefs::emitFull(self, "spell", Sp\ID, "(unnamed)", "Name", "(empty)", "spell has no Name", "warning", "empty-field")
        EndIf

        // Spell with no script -- casting does nothing
        If Sp\Script$ = ""
            BrokenRefs::emitFull(self, "spell", Sp\ID, Sp\Name$, "Script", "(empty)", "spell has no Script bound -- cast does nothing", "warning", "spell-config")
        EndIf

        // Script bound but method empty
        If Sp\Script$ <> "" And Sp\SMethod$ = ""
            BrokenRefs::emitFull(self, "spell", Sp\ID, Sp\Name$, "SMethod", "(empty)", "spell has Bound script but no Method", "warning", "spell-config")
        EndIf
    End Method


    // -------------------------------------------------------------------------
    // scanZoneContent -- orphan zone (no portals, no spawns, no triggers
    // = nothing to do there).
    // -------------------------------------------------------------------------
    Method scanZoneContent(Ar.Area)
        Local hasPortal% = False
        Local hasSpawn%  = False
        Local hasTrigger% = False
        Local i%
        For i = 0 To 99
            If Ar\PortalName$[i] <> "" Then hasPortal = True : Exit
        Next
        For i = 0 To 999
            If Ar\SpawnActor[i] > 0 Then hasSpawn = True : Exit
        Next
        For i = 0 To 149
            If Ar\TriggerScript$[i] <> "" Then hasTrigger = True : Exit
        Next

        If hasPortal = False And hasSpawn = False And hasTrigger = False
            BrokenRefs::emitFull(self, "zone", Handle(Ar), Ar\Name$, "(contents)", "0 portals, 0 spawns, 0 triggers", "orphan zone -- nothing for a player to do here", "info", "orphan-zone")
        EndIf
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
        // Back-compat shape: existing scanActor / scanZone callers use this
        // shorter signature and emit broken-ref category at error severity.
        BrokenRefs::emitFull(self, sourceKind, sourceRefID, sourceLabel, fieldDesc, badValue, diagnosis, "error", "broken-ref")
    End Method


    Method emitFull(sourceKind$, sourceRefID%, sourceLabel$, fieldDesc$, badValue$, diagnosis$, severity$, category$)
        Local r.BrokenRef = New BrokenRef()
        r\SourceKind = sourceKind
        r\SourceRefID = sourceRefID
        r\SourceLabel = sourceLabel
        r\FieldDesc = fieldDesc
        r\BadValue = badValue
        r\Diagnosis = diagnosis
        r\Severity = severity
        r\Category = category
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
        Local clicked% = Loom_MouseClicked()

        Local modalX% = (sw - BROKENREFS_MODAL_W) / 2
        Local modalY% = (sh - BROKENREFS_MODAL_H) / 3

        LoomShadowCard(modalX, modalY, BROKENREFS_MODAL_W, BROKENREFS_MODAL_H)
        LoomFill(modalX, modalY, BROKENREFS_MODAL_W, BROKENREFS_MODAL_H, LOOM_STONE_850_R, LOOM_STONE_850_G, LOOM_STONE_850_B)
        LoomBorder(modalX, modalY, BROKENREFS_MODAL_W, BROKENREFS_MODAL_H, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
        LoomBorder(modalX + 1, modalY + 1, BROKENREFS_MODAL_W - 2, BROKENREFS_MODAL_H - 2, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        LoomFill(modalX, modalY, BROKENREFS_MODAL_W, 3, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)

        // Header in display font. Title broadened from "Broken References" to
        // "Issues" now that the modal surfaces multiple categories of
        // validation (broken refs / empty fields / playability gaps /
        // weapon config / spell config / orphan zones).
        Local headerTxt$ = "ISSUES  |  " + Str(self\entryCount)
        If self\entryCount >= BROKENREFS_MAX_ENTRIES Then headerTxt = headerTxt + "+ (capped)"
        LoomTheme_UseDisplay()
        LoomText(modalX + BROKENREFS_PAD, modalY + 6, headerTxt, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
        LoomTheme_UseBody()

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
            LoomText(rx, listY + 12, "No issues. Project is clean.", LOOM_SUCCESS_R, LOOM_SUCCESS_G, LOOM_SUCCESS_B)
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

        // Severity left-rail (4px) -- red error / orange warning / blue info.
        // Defaults to red for back-compat entries that pre-date the
        // severity field (Severity = "" reads as error). Avoids reassigning
        // Method-scope Locals from inside nested If branches per the
        // Strict-mode trap; instead each branch fills the rail directly.
        If r\Severity = "warning"
            LoomFill(rx, ry, 4, BROKENREFS_ROW_H, LOOM_WARNING_R, LOOM_WARNING_G, LOOM_WARNING_B)
        Else If r\Severity = "info"
            LoomFill(rx, ry, 4, BROKENREFS_ROW_H, LOOM_ARCANE_500_R, LOOM_ARCANE_500_G, LOOM_ARCANE_500_B)
        Else
            LoomFill(rx, ry, 4, BROKENREFS_ROW_H, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
        EndIf

        // Source: kind + label (shifted right past the rail)
        LoomText(rx + 12, ry + 4, r\SourceKind + "  " + r\SourceLabel, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        // Field + bad value (colored by severity, same conditional)
        Local mid$ = r\FieldDesc + " = " + Chr(34) + BrokenRefs::truncate(self, r\BadValue, 20) + Chr(34)
        If r\Severity = "warning"
            LoomText(rx + 260, ry + 4, mid, LOOM_WARNING_R, LOOM_WARNING_G, LOOM_WARNING_B)
        Else If r\Severity = "info"
            LoomText(rx + 260, ry + 4, mid, LOOM_ARCANE_500_R, LOOM_ARCANE_500_G, LOOM_ARCANE_500_B)
        Else
            LoomText(rx + 260, ry + 4, mid, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
        EndIf

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
