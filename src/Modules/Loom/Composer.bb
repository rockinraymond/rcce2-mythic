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
//   When edit mode is active for a field, mutates the underlying type instance
//   on commit (Enter or Save button). Sets the corresponding per-tab *Saved
//   global to False so GUE sees Loom's edits as dirty too. Save dispatch
//   (Composer::commitSaveForKind) calls GUE's existing Save* serializers
//   (SaveSpells, SaveItems, ...) so the on-disk format stays canonical.
//
// Architecture: Type with Methods, called as `Composer::method(self, args)`.


Const CMP_W           = 380
Const CMP_TOP         = LOOM_TOP_RIBBON_H + 56   // matches BR_TOP_RIBBON (84)
Const CMP_BOT_PAD     = 36     // matches BR_BOT_RIBBON
Const CMP_PAD         = 16
Const CMP_ROW_H       = 22
Const CMP_CHIP_H      = 26

// Save button (top-right of the composer title block).
Const CMP_SAVE_BTN_W  = 70
Const CMP_SAVE_BTN_H  = 22

// Delete button (top-right, immediately left of Save). Two-click arm/confirm
// pattern -- first click arms (button turns red, footer hint changes),
// second click within CMP_DELETE_ARM_MS commits the delete. Click anywhere
// else (or wait out the window) cancels.
Const CMP_DELETE_BTN_W  = 24
Const CMP_DELETE_BTN_H  = 22
Const CMP_DELETE_ARM_MS = 4000

// Discard button (top-right, immediately left of Delete). Reverts the
// current kind's in-memory state by re-running its loader against the
// on-disk file. Same shape as the Delete arm/confirm so accidents are
// avoided when there's unsaved work.
Const CMP_DISCARD_BTN_W  = 60
Const CMP_DISCARD_BTN_H  = 22

// Edit-buffer cursor blink rate (ms). MilliSecs() Mod CMP_CURSOR_PERIOD < half = visible.
Const CMP_CURSOR_PERIOD = 1000


// =============================================================================
// Composer -- right-side property panel.
// =============================================================================
Type Composer
    Field threads.Threads      // shared focus state, set by caller

    // Per-frame chip-click latch -- the per-kind body renderers set this when
    // any thread chip consumed a click, and renderAndUpdate returns it so the
    // caller can react (e.g. log it, refresh another surface).
    Field chipHit%

    // Edit-buffer state. editKind = "" means no edit in progress.
    // (kind, refID, fieldId) together identify which field of which entity
    // the user is currently typing into. editBuffer is the in-progress value
    // shown with a blinking cursor; on Enter it's written to the entity's
    // field via commitEdit.
    Field editKind$
    Field editRefID%
    Field editFieldId$
    Field editBuffer$
    Field editOldValue$    // snapshot at beginEdit, used by commitEdit
                           // to record the Timeline before/after pair

    // Delete-arm state. When the user clicks Delete, we record the kind/
    // refID and a timestamp. A second click within CMP_DELETE_ARM_MS on
    // the same (kind, refID) commits; clicking anywhere else cancels.
    Field deleteArmKind$
    Field deleteArmRefID%
    Field deleteArmAt%

    // Discard-arm state -- same shape as delete-arm. Only the kind matters
    // (discard reloads the whole kind from disk, not a single entity).
    Field discardArmKind$
    Field discardArmAt%

    // Palette reference -- set by setPalette from Loom.bb at construction.
    // Used by chipRow to dispatch picker-mode opens on right-click of a
    // thread chip.
    Field palette.Palette


    Method create.Composer(threads.Threads)
        self\threads = threads
        self\chipHit = False
        self\editKind = ""
        self\editRefID = 0
        self\editFieldId = ""
        self\editBuffer = ""
        self\editOldValue = ""
        self\deleteArmKind = ""
        self\deleteArmRefID = 0
        self\deleteArmAt = 0
        self\discardArmKind = ""
        self\discardArmAt = 0
        self\palette = Null
        Return self
    End Method


    // -------------------------------------------------------------------------
    // setPalette -- injection point from Loom.bb so chipRow can dispatch
    // picker-mode opens. Called once at construction.
    // -------------------------------------------------------------------------
    Method setPalette(palette.Palette)
        self\palette = palette
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
    // isEditing -- read accessor for the outer Loom frame so it knows the
    // composer is currently consuming keystrokes (and the Browser's filter
    // input must stay quiet).
    // -------------------------------------------------------------------------
    Method isEditing%()
        If self\editKind <> "" Then Return True
        Return False
    End Method


    // -------------------------------------------------------------------------
    // renderAndUpdate -- per-frame paint + chip hit-test. No-op when nothing
    // is focused. Returns True if any chip was clicked this frame.
    // -------------------------------------------------------------------------
    Method renderAndUpdate%(sw%, sh%)
        If self\threads\focusKind = "" Then Return False

        Local kind$ = self\threads\focusKind
        Local refID% = self\threads\focusID

        // If focus changed while editing, cancel the pending edit (don't
        // mutate a stale target). Same kind+id keeps the edit active.
        If self\editKind <> ""
            If self\editKind <> kind Or self\editRefID <> refID
                Composer::cancelEdit(self)
            EndIf
        EndIf

        // Drain keyboard into editBuffer if an edit is active. Consumes Esc
        // (cancel) and Enter (commit) so Loom's outer Esc handler doesn't
        // see them on the same frame.
        If self\editKind <> ""
            Composer::pumpKeyboard(self)
        EndIf

        Local mx% = MouseX()
        Local my% = MouseY()
        Local clicked% = MouseHit(1)
        // Right-click captured once per frame; thread chips read it via
        // chipRow -> renderChip to dispatch picker-mode opens. Capture
        // here (not in chipRow) so MouseHit(2)'s consume-once semantics
        // don't make only the first chip see the press.
        Local rightClicked% = MouseHit(2)

        Local x% = sw - CMP_W
        Local y% = CMP_TOP
        Local w% = CMP_W
        Local h% = sh - CMP_TOP - CMP_BOT_PAD

        // Panel chrome -- brass left rule signals the primary surface.
        // Subtle stone-850 -> stone-900 gradient gives the panel depth
        // rather than reading as a flat slab pasted onto the browser.
        LoomGradientV(x, y, w, h, LOOM_STONE_850_R, LOOM_STONE_850_G, LOOM_STONE_850_B, LOOM_STONE_900_R, LOOM_STONE_900_G, LOOM_STONE_900_B)
        LoomBorder(x, y, w, h, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        LoomFill(x, y, 3, h, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

        // Title block
        Local kindLabel$ = Composer::kindLabel(self, kind)
        Local entityName$ = Threads::lookupName(self\threads, kind, refID)
        If entityName = "" Then entityName = "(unknown)"

        // Dirty asterisk -- prefix the entity name when its kind has unsaved
        // edits. Visible regardless of who made the edit (Loom or GUE).
        Local dirty% = Composer::isDirtyForKind(self, kind)
        If dirty = True Then entityName = "* " + entityName

        LoomText(x + CMP_PAD, y + CMP_PAD, kindLabel, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomText(x + CMP_PAD, y + CMP_PAD + 16, entityName, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        // Top-right action cluster (right-to-left): Save / Discard / Delete.
        // Save + Discard only render when there's pending dirty state for
        // this kind. Delete always renders (any focused entity can be
        // deleted; arm/confirm guards against accidents).
        Local btnY% = y + CMP_PAD - 2
        Local btnX% = x + w - CMP_SAVE_BTN_W - CMP_PAD
        If dirty = True
            Composer::drawSaveButton(self, btnX, btnY, mx, my, clicked, kind)
            btnX = btnX - CMP_DISCARD_BTN_W - 6
            Composer::drawDiscardButton(self, btnX, btnY, mx, my, clicked, kind)
            btnX = btnX - CMP_DELETE_BTN_W - 6
        Else
            btnX = btnX + CMP_SAVE_BTN_W - CMP_DELETE_BTN_W
        EndIf
        Composer::drawDeleteButton(self, btnX, btnY, mx, my, clicked, kind, refID)

        LoomHRule(x + CMP_PAD, y + CMP_PAD + 38, w - CMP_PAD * 2, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)

        // Body -- per-kind render dispatch
        Local bodyY% = y + CMP_PAD + 50
        Local bodyH% = h - (bodyY - y) - 24
        self\chipHit = False

        If kind = "actor"
            Composer::renderActor(self, x, bodyY, w, bodyH, mx, my, clicked, rightClicked)
        Else If kind = "item"
            Composer::renderItem(self, x, bodyY, w, bodyH, mx, my, clicked)
        Else If kind = "spell"
            Composer::renderSpell(self, x, bodyY, w, bodyH, mx, my, clicked)
        Else If kind = "zone"
            Composer::renderZone(self, x, bodyY, w, bodyH, mx, my, clicked, rightClicked)
        Else If kind = "faction"
            Composer::renderFaction(self, x, bodyY, w, bodyH, mx, my, clicked, rightClicked)
        Else If kind = "animset"
            Composer::renderAnimSet(self, x, bodyY, w, bodyH, mx, my, clicked, rightClicked)
        EndIf

        // Footer: back-stack hint (or edit-mode hint when editing). Two
        // separate Ifs rather than If/Else-If to dodge the BlitzForge
        // Else-If scope leak (issue #61).
        Local stackSize% = ListSize(self\threads\backStack)
        Local footMsg$ = "Esc returns to browser"
        If self\editKind = "" And stackSize > 0
            footMsg = "Esc walks back  |  " + Str(stackSize) + " in trail"
        EndIf
        If self\editKind <> ""
            footMsg = "Enter to commit  |  Esc to cancel edit"
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


    // -------------------------------------------------------------------------
    // editableIntRow / editableFloatRow / toggleRow -- thin wrappers over
    // editableRow that convert the stored numeric value to a string for
    // display + buffer-seed. On commit, writeField parses the buffer and
    // ignores bad input (leaves the stored value unchanged).
    //
    // toggleRow is special: there's no edit buffer because a bool only has
    // two states. Click the value cell to flip in-place, mark dirty.
    // -------------------------------------------------------------------------
    Method editableIntRow%(panelX%, panelW%, rowY%, label$, kind$, refID%, fieldId$, storedValue%, mx%, my%, clicked%)
        Return Composer::editableRow(self, panelX, panelW, rowY, label, kind, refID, fieldId, Str(storedValue), mx, my, clicked)
    End Method


    Method editableFloatRow%(panelX%, panelW%, rowY%, label$, kind$, refID%, fieldId$, storedValue#, mx%, my%, clicked%)
        Return Composer::editableRow(self, panelX, panelW, rowY, label, kind, refID, fieldId, Composer::formatFloat(self, storedValue), mx, my, clicked)
    End Method


    // toggleRow -- click the value cell to flip the stored bool. Returns next Y.
    Method toggleRow%(panelX%, panelW%, rowY%, label$, kind$, refID%, fieldId$, storedValue%, mx%, my%, clicked%)
        Local valX% = panelX + CMP_PAD + 120
        Local valY% = rowY - 3
        Local valW% = panelW - CMP_PAD * 2 - 120
        Local valH% = CMP_ROW_H
        Local hovered% = (mx >= valX And mx < valX + valW And my >= valY And my < valY + valH)

        If hovered = True
            LoomFill(valX, valY, valW, valH, LOOM_STONE_800_R, LOOM_STONE_800_G, LOOM_STONE_800_B)
            LoomBorder(valX, valY, valW, valH, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        EndIf

        LoomText(panelX + CMP_PAD, rowY, label, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

        // Mini toggle indicator on the right of the value cell -- a small
        // brass pill that's filled when True, hollow when False. Affordance
        // makes the click target read as a switch.
        Local pillW% = 30
        Local pillH% = 14
        Local pillX% = valX + 4
        Local pillY% = valY + (valH - pillH) / 2
        If storedValue = True
            LoomFill(pillX, pillY, pillW, pillH, LOOM_ARCANE_500_R, LOOM_ARCANE_500_G, LOOM_ARCANE_500_B)
            LoomFill(pillX + pillW - 12, pillY + 2, 10, pillH - 4, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
        Else
            LoomBorder(pillX, pillY, pillW, pillH, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
            LoomFill(pillX + 2, pillY + 2, 10, pillH - 4, LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)
        EndIf

        // Label text
        Local labelTxt$ = "No"
        If storedValue = True Then labelTxt = "Yes"
        LoomText(pillX + pillW + 8, rowY, labelTxt, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        If hovered And clicked
            // Cancel any pending text-edit on a different field first.
            If self\editKind <> "" Then Composer::commitEdit(self)
            Local newVal% = True
            If storedValue = True Then newVal = False
            Composer::writeField(self, kind, refID, fieldId, Str(newVal))
            Composer::markDirtyForKind(self, kind)
            Timeline_RecordToggle(kind, refID, fieldId, Str(storedValue), Str(newVal), Threads::lookupName(self\threads, kind, refID))
            WorldCache_Invalidate()
            WriteLog(LoomLog, "Composer: toggled " + kind + "#" + Str(refID) + " " + fieldId + " -> " + Str(newVal))
        EndIf

        Return rowY + CMP_ROW_H
    End Method


    // -------------------------------------------------------------------------
    // editableRow -- like row(), but the value cell is clickable to begin
    // editing. When this exact (kind, refID, fieldId) is active, the cell
    // shows the edit buffer with a blinking cursor instead of the stored
    // value. Click elsewhere or Enter to commit; Esc to cancel.
    //
    // Returns the next Y.
    // -------------------------------------------------------------------------
    Method editableRow%(panelX%, panelW%, rowY%, label$, kind$, refID%, fieldId$, storedValue$, mx%, my%, clicked%)
        Local valX% = panelX + CMP_PAD + 120
        Local valY% = rowY - 3
        Local valW% = panelW - CMP_PAD * 2 - 120
        Local valH% = CMP_ROW_H

        Local active% = (self\editKind = kind And self\editRefID = refID And self\editFieldId = fieldId)
        Local hovered% = (mx >= valX And mx < valX + valW And my >= valY And my < valY + valH)

        // Background + border. Editing > hover > flat.
        If active = True
            LoomFill(valX, valY, valW, valH, LOOM_STONE_700_R, LOOM_STONE_700_G, LOOM_STONE_700_B)
            LoomBorder(valX, valY, valW, valH, LOOM_ARCANE_500_R, LOOM_ARCANE_500_G, LOOM_ARCANE_500_B)
        Else If hovered = True
            LoomFill(valX, valY, valW, valH, LOOM_STONE_800_R, LOOM_STONE_800_G, LOOM_STONE_800_B)
            LoomBorder(valX, valY, valW, valH, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        EndIf

        // Label
        LoomText(panelX + CMP_PAD, rowY, label, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

        // Value (buffer when editing; stored value otherwise)
        Local shown$
        If active = True
            shown = self\editBuffer
        Else
            shown = storedValue
        EndIf
        LoomText(valX + 4, rowY, shown, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        // Blinking cursor at end of buffer when editing
        If active = True
            If (MilliSecs() Mod CMP_CURSOR_PERIOD) < (CMP_CURSOR_PERIOD / 2)
                Local cursorX% = valX + 4 + StringWidth(self\editBuffer)
                LoomFill(cursorX, rowY, 2, 14, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
            EndIf
        EndIf

        // Click to begin / commit. Click on the value rect while NOT editing
        // begins; click outside while editing commits (handled here by no-op
        // -- the next-frame check at top of renderAndUpdate sees mismatched
        // focus and cancels, but we want commit-on-click-elsewhere).
        If hovered And clicked And active = False
            Composer::beginEdit(self, kind, refID, fieldId, storedValue)
        Else If clicked And active = True And hovered = False
            Composer::commitEdit(self)
        EndIf

        Return rowY + CMP_ROW_H
    End Method


    // -------------------------------------------------------------------------
    // beginEdit -- enter edit mode for a specific (kind, refID, fieldId).
    // Seeds the buffer with the current stored value.
    // -------------------------------------------------------------------------
    Method beginEdit(kind$, refID%, fieldId$, currentValue$)
        // Commit any pending edit on a different field before switching.
        If self\editKind <> ""
            Composer::commitEdit(self)
        EndIf
        self\editKind = kind
        self\editRefID = refID
        self\editFieldId = fieldId
        self\editBuffer = currentValue
        self\editOldValue = currentValue
        FlushKeys      // discard any buffered keystrokes from the click itself
        WriteLog(LoomLog, "Composer: begin edit " + kind + "#" + Str(refID) + " " + fieldId + " = " + Chr(34) + currentValue + Chr(34))
    End Method


    // -------------------------------------------------------------------------
    // commitEdit -- write the buffer to the target field and mark the kind's
    // *Saved global False. Clears edit state.
    // -------------------------------------------------------------------------
    Method commitEdit()
        If self\editKind = "" Then Return

        Local k$ = self\editKind
        Local id% = self\editRefID
        Local fid$ = self\editFieldId
        Local val$ = self\editBuffer
        Local oldVal$ = self\editOldValue

        Composer::writeField(self, k, id, fid, val)
        Composer::markDirtyForKind(self, k)

        // Record only when the value actually changed (avoids spamming
        // the timeline with no-op commits from click-away-without-typing).
        If val <> oldVal
            Timeline_RecordEdit(k, id, fid, oldVal, val, Threads::lookupName(self\threads, k, id))
            // Reference-field edits + faction renames can change the
            // broken-ref count, and any edit changes "what to show in
            // the recents label" etc. Conservatively invalidate the
            // shared cache; the next ribbon paint recomputes.
            WorldCache_Invalidate()
        EndIf

        WriteLog(LoomLog, "Composer: commit " + k + "#" + Str(id) + " " + fid + " <- " + Chr(34) + val + Chr(34))

        self\editKind = ""
        self\editRefID = 0
        self\editFieldId = ""
        self\editBuffer = ""
        self\editOldValue = ""
    End Method


    // -------------------------------------------------------------------------
    // cancelEdit -- drop the buffer; field stays at its previous stored value.
    // -------------------------------------------------------------------------
    Method cancelEdit()
        If self\editKind = "" Then Return
        WriteLog(LoomLog, "Composer: cancel edit " + self\editKind + "#" + Str(self\editRefID) + " " + self\editFieldId)
        self\editKind = ""
        self\editRefID = 0
        self\editFieldId = ""
        self\editBuffer = ""
        self\editOldValue = ""
    End Method


    // -------------------------------------------------------------------------
    // pumpKeyboard -- called per-frame when editing. Drains the keyboard
    // input queue into editBuffer, handles Enter (commit) / Esc (cancel) /
    // Backspace.
    // -------------------------------------------------------------------------
    Method pumpKeyboard()
        // Backspace
        If KeyHit(14) And Len(self\editBuffer) > 0
            self\editBuffer = Left$(self\editBuffer, Len(self\editBuffer) - 1)
        EndIf

        // Enter -- commit
        If KeyHit(28)
            Composer::commitEdit(self)
            Return
        EndIf

        // Esc -- cancel (consumed here so Loom's outer Esc doesn't fire)
        If KeyHit(1)
            Composer::cancelEdit(self)
            Return
        EndIf

        // Printable chars (drain the GetKey queue). Filter to ASCII 32..126
        // so we don't get control chars in the buffer.
        Local k% = GetKey()
        While k > 0
            If k >= 32 And k <= 126
                self\editBuffer = self\editBuffer + Chr(k)
            EndIf
            k = GetKey()
        Wend
    End Method


    // -------------------------------------------------------------------------
    // writeField -- dispatch table mapping (kind, fieldId) -> in-memory write.
    // Only fields explicitly handled here are editable; unknown combinations
    // are no-ops (logged for diagnostics).
    //
    // Numeric fields use parseInt / parseFloat below; bad input leaves the
    // stored value alone (parseX returns the supplied default which is the
    // current stored value at call time, but we read+write in one expression
    // so the default flows correctly).
    //
    // Bool fields are toggled via toggleRow which writes "0"/"1" as the
    // value string; we just compare to "1".
    // -------------------------------------------------------------------------
    Method writeField(kind$, refID%, fieldId$, value$)
        // ---- SPELL ----------------------------------------------------------
        If kind = "spell"
            If refID < 0 Or refID > 65534 Then Return
            Local S.Spell = SpellsList(refID)
            If S = Null Then Return
            If fieldId = "name"           Then S\Name$ = value         : Return
            If fieldId = "description"    Then S\Description$ = value  : Return
            If fieldId = "recharge_ms"    Then S\RechargeTime = Composer::parseIntClamped(self, value, S\RechargeTime, 0, 3600000) : Return
            If fieldId = "script"         Then S\Script$ = value       : Return
            If fieldId = "smethod"        Then S\SMethod$ = value      : Return
            If fieldId = "race"           Then S\ExclusiveRace$ = value  : Return
            If fieldId = "class"          Then S\ExclusiveClass$ = value : Return
        EndIf

        // ---- ITEM -----------------------------------------------------------
        If kind = "item"
            If refID < 0 Or refID > 65534 Then Return
            Local I.Item = ItemList(refID)
            If I = Null Then Return
            If fieldId = "name"           Then I\Name$ = value         : Return
            If fieldId = "value"          Then I\Value = Composer::parseIntClamped(self, value, I\Value, 0, 2000000000) : Return
            If fieldId = "mass"           Then I\Mass  = Composer::parseIntClamped(self, value, I\Mass,  0, 2000000000) : Return
            If fieldId = "weapon_damage"  Then I\WeaponDamage = Composer::parseIntClamped(self, value, I\WeaponDamage, 0, 2000000000) : Return
            If fieldId = "armour_level"   Then I\ArmourLevel  = Composer::parseIntClamped(self, value, I\ArmourLevel,  0, 2000000000) : Return
            If fieldId = "range"          Then I\Range#       = Composer::parseFloatClamped(self, value, I\Range#, 0.0, 100000.0) : Return
            If fieldId = "script"         Then I\Script$ = value       : Return
            If fieldId = "smethod"        Then I\SMethod$ = value      : Return
            If fieldId = "race"           Then I\ExclusiveRace$ = value  : Return
            If fieldId = "class"          Then I\ExclusiveClass$ = value : Return
            If fieldId = "stackable"      Then I\Stackable   = (value = "1") : Return
            If fieldId = "breakable"      Then I\TakesDamage = (value = "1") : Return
        EndIf

        // ---- ACTOR ----------------------------------------------------------
        If kind = "actor"
            If refID < 0 Or refID > 65535 Then Return
            Local A.Actor = ActorList(refID)
            If A = Null Then Return
            If fieldId = "race"           Then A\Race$ = value         : Return
            If fieldId = "class"          Then A\Class$ = value        : Return
            If fieldId = "description"    Then A\Description$ = value  : Return
            If fieldId = "scale"          Then A\Scale#      = Composer::parseFloatClamped(self, value, A\Scale#,      0.01, 100.0) : Return
            If fieldId = "xpmult"         Then A\XPMultiplier = Composer::parseIntClamped(self, value, A\XPMultiplier, 0, 1000000) : Return
            If fieldId = "aggressiveness" Then A\Aggressiveness  = Composer::parseIntClamped(self, value, A\Aggressiveness,  0, 3) : Return
            If fieldId = "agg_range"      Then A\AggressiveRange = Composer::parseIntClamped(self, value, A\AggressiveRange, 0, 100000) : Return
            If fieldId = "genders"        Then A\Genders         = Composer::parseIntClamped(self, value, A\Genders,         0, 3) : Return
            If fieldId = "trade_mode"     Then A\TradeMode       = Composer::parseIntClamped(self, value, A\TradeMode,       0, 2) : Return
            If fieldId = "playable"       Then A\Playable    = (value = "1") : Return
            If fieldId = "rideable"       Then A\Rideable    = (value = "1") : Return
            // Reference fields edited via the palette picker. The picker
            // writes the chosen entity's refID as a string; clamp to the
            // valid slot range for the kind.
            If fieldId = "default_faction" Then A\DefaultFaction = Composer::parseIntClamped(self, value, A\DefaultFaction, 0, 99)   : Return
            If fieldId = "manim_set"       Then A\MAnimationSet  = Composer::parseIntClamped(self, value, A\MAnimationSet,  0, 999)  : Return
            If fieldId = "fanim_set"       Then A\FAnimationSet  = Composer::parseIntClamped(self, value, A\FAnimationSet,  0, 999)  : Return
        EndIf

        // ---- ZONE -----------------------------------------------------------
        If kind = "zone"
            Local Ar.Area = Object.Area(refID)
            If Ar = Null Then Return
            If fieldId = "name"           Then Ar\Name$ = value          : Return
            If fieldId = "gravity"        Then Ar\Gravity = Composer::parseIntClamped(self, value, Ar\Gravity, 0, 1000) : Return
            If fieldId = "entry_script"   Then Ar\EntryScript$ = value   : Return
            If fieldId = "exit_script"    Then Ar\ExitScript$ = value    : Return
            If fieldId = "weather_link"   Then Ar\WeatherLink$ = value   : Return
            If fieldId = "outdoors"       Then Ar\Outdoors = (value = "1") : Return
            If fieldId = "pvp"            Then Ar\PvP      = (value = "1") : Return
            // Portal-target reference fields: editFieldId = "portal_<i>".
            // The picker passes the chosen zone's Name as the value (the
            // portal stores by name, not handle).
            If Left$(fieldId, 7) = "portal_"
                Local portIdx% = Int(Mid$(fieldId, 8))
                If portIdx >= 0 And portIdx <= 99 Then Ar\PortalLinkArea$[portIdx] = value
                Return
            EndIf
        EndIf

        // ---- FACTION --------------------------------------------------------
        If kind = "faction"
            If refID < 0 Or refID > 99 Then Return
            // SetFactionName lives in Actors.bb (non-Strict) -- direct write
            // to the FactionNames$ global from this Strict file would error
            // per the Dim-inside-Method gotcha.
            If fieldId = "name" Then SetFactionName(refID, value) : Return
        EndIf

        // ---- ANIMSET --------------------------------------------------------
        If kind = "animset"
            // AnimSet is iterated, not array-indexed; walk to the matching ID.
            Local As2.AnimSet
            For As2 = Each AnimSet
                If As2\ID = refID
                    If fieldId = "name" Then As2\Name$ = value : Return
                    Exit
                EndIf
            Next
        EndIf

        WriteLog(LoomLog, "Composer: writeField -- no handler for " + kind + "." + fieldId)
    End Method


    // -------------------------------------------------------------------------
    // parseIntClamped / parseFloatClamped -- numeric parsers used by
    // writeField for editable int / float fields. Empty input returns the
    // fallback (cancels the edit); otherwise BlitzForge's Int() / Float()
    // handles parsing -- they return 0 for pure-garbage strings. The result
    // is clamped into [lo, hi] so an editing typo can't blow a field out to
    // range-breaking values.
    //
    // Rationale for skipping a strict "is this digits" pre-check: the
    // Strict-mode "reassigning a Method-scope Local from inside nested
    // If/For blocks" trap (architecture.md "Known BlitzForge gotchas") makes
    // a character-class loop awkward, and the cost of accepting "abc" -> 0
    // -> clamp-to-lo is minimal -- the user sees the wrong value and
    // re-edits. The clamp is the real protection here, not the validator.
    // -------------------------------------------------------------------------
    Method parseIntClamped%(s$, fallback%, lo%, hi%)
        If Trim$(s) = "" Then Return fallback
        Local v% = Int(s)
        If v < lo Then v = lo
        If v > hi Then v = hi
        Return v
    End Method


    Method parseFloatClamped#(s$, fallback#, lo#, hi#)
        If Trim$(s) = "" Then Return fallback
        Local v# = Float(s)
        If v# < lo# Then v# = lo#
        If v# > hi# Then v# = hi#
        Return v#
    End Method


    // -------------------------------------------------------------------------
    // isDirtyForKind -- read the per-kind *Saved global, return True if
    // there are unsaved edits.
    // -------------------------------------------------------------------------
    Method isDirtyForKind%(kind$)
        If kind = "spell"   Then Return Not SpellsSaved
        If kind = "item"    Then Return Not ItemsSaved
        If kind = "actor"   Then Return Not ActorsSaved
        If kind = "faction" Then Return Not FactionsSaved
        If kind = "zone"    Then Return Not ZoneSaved
        If kind = "animset" Then Return Not AnimsSaved
        Return False
    End Method


    // -------------------------------------------------------------------------
    // markDirtyForKind -- set the per-kind *Saved global to False after
    // an in-memory edit.
    // -------------------------------------------------------------------------
    Method markDirtyForKind(kind$)
        If kind = "spell"   Then SpellsSaved = False
        If kind = "item"    Then ItemsSaved = False
        If kind = "actor"   Then ActorsSaved = False
        If kind = "faction" Then FactionsSaved = False
        If kind = "zone"    Then ZoneSaved = False
        If kind = "animset" Then AnimsSaved = False
    End Method


    // -------------------------------------------------------------------------
    // commitSaveForKind -- persist in-memory state for the given kind to
    // disk via GUE's existing Save* serializers, then clear the dirty flag.
    //
    // Zone is the special case: each Area has its own .dat file (Areas/<name>.dat)
    // so ServerSaveArea takes an Area instance instead of a file path. We
    // save only the focused zone, not the entire collection.
    // -------------------------------------------------------------------------
    Method commitSaveForKind(kind$)
        If kind = "spell"
            Local okS% = SaveSpells("Data\Server Data\Spells.dat")
            If okS = False
                WriteLog(LoomLog, "Composer: SaveSpells FAILED")
                Toast_Show("Save Spells FAILED", "danger")
                Return
            EndIf
            SpellsSaved = True
            WriteLog(LoomLog, "Composer: saved Spells.dat")
            Toast_Show("Saved Spells.dat", "success")
            Return
        EndIf

        If kind = "item"
            Local okI% = SaveItems("Data\Server Data\Items.dat")
            If okI = False
                WriteLog(LoomLog, "Composer: SaveItems FAILED")
                Toast_Show("Save Items FAILED", "danger")
                Return
            EndIf
            ItemsSaved = True
            WriteLog(LoomLog, "Composer: saved Items.dat")
            Toast_Show("Saved Items.dat", "success")
            Return
        EndIf

        If kind = "actor"
            Local okA% = SaveActors("Data\Server Data\Actors.dat")
            If okA = False
                WriteLog(LoomLog, "Composer: SaveActors FAILED")
                Toast_Show("Save Actors FAILED", "danger")
                Return
            EndIf
            ActorsSaved = True
            WriteLog(LoomLog, "Composer: saved Actors.dat")
            Toast_Show("Saved Actors.dat", "success")
            Return
        EndIf

        If kind = "faction"
            Local okF% = SaveFactions("Data\Server Data\Factions.dat")
            If okF = False
                WriteLog(LoomLog, "Composer: SaveFactions FAILED")
                Toast_Show("Save Factions FAILED", "danger")
                Return
            EndIf
            FactionsSaved = True
            WriteLog(LoomLog, "Composer: saved Factions.dat")
            Toast_Show("Saved Factions.dat", "success")
            Return
        EndIf

        If kind = "animset"
            Local okM% = SaveAnimSets("Data\Game Data\Animations.dat")
            If okM = False
                WriteLog(LoomLog, "Composer: SaveAnimSets FAILED")
                Toast_Show("Save Animations FAILED", "danger")
                Return
            EndIf
            AnimsSaved = True
            WriteLog(LoomLog, "Composer: saved Animations.dat")
            Toast_Show("Saved Animations.dat", "success")
            Return
        EndIf

        If kind = "zone"
            Local Ar.Area = Object.Area(self\threads\focusID)
            If Ar = Null
                WriteLog(LoomLog, "Composer: ServerSaveArea -- focused area handle is stale")
                Toast_Show("Save Zone failed (stale handle)", "danger")
                Return
            EndIf
            ServerSaveArea(Ar)
            // ServerSaveArea is void; we trust it. The atomic-write
            // discipline is owned by the serializer itself.
            ZoneSaved = True
            WriteLog(LoomLog, "Composer: saved zone " + Ar\Name$)
            Toast_Show("Saved zone " + Ar\Name$, "success")
            Return
        EndIf

        WriteLog(LoomLog, "Composer: commitSaveForKind -- no handler for " + kind)
    End Method


    // -------------------------------------------------------------------------
    // drawDeleteButton -- destructive action with arm/confirm. First click
    // arms (button turns red, footer hint updates); second click on the
    // same focus within CMP_DELETE_ARM_MS commits via EntityFactory_Delete.
    // Focus change / other-button click / timeout cancels.
    // -------------------------------------------------------------------------
    Method drawDeleteButton(btnX%, btnY%, mx%, my%, clicked%, kind$, refID%)
        Local hovered% = (mx >= btnX And mx < btnX + CMP_DELETE_BTN_W And my >= btnY And my < btnY + CMP_DELETE_BTN_H)

        // Has the arm window expired? If so, drop it.
        If self\deleteArmKind <> "" And (MilliSecs() - self\deleteArmAt) > CMP_DELETE_ARM_MS
            self\deleteArmKind = ""
            self\deleteArmRefID = 0
            self\deleteArmAt = 0
        EndIf

        Local armed% = (self\deleteArmKind = kind And self\deleteArmRefID = refID)

        If armed = True
            LoomFill(btnX, btnY, CMP_DELETE_BTN_W, CMP_DELETE_BTN_H, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
            LoomBorder(btnX, btnY, CMP_DELETE_BTN_W, CMP_DELETE_BTN_H, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
            LoomText(btnX + 7, btnY + 4, "X", LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
        Else If hovered = True
            LoomFill(btnX, btnY, CMP_DELETE_BTN_W, CMP_DELETE_BTN_H, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
            LoomBorder(btnX, btnY, CMP_DELETE_BTN_W, CMP_DELETE_BTN_H, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
            LoomText(btnX + 8, btnY + 4, "X", LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
        Else
            LoomFill(btnX, btnY, CMP_DELETE_BTN_W, CMP_DELETE_BTN_H, LOOM_STONE_800_R, LOOM_STONE_800_G, LOOM_STONE_800_B)
            LoomBorder(btnX, btnY, CMP_DELETE_BTN_W, CMP_DELETE_BTN_H, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
            LoomText(btnX + 8, btnY + 4, "X", LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
        EndIf

        If hovered And clicked
            If armed = True
                // Commit
                EntityFactory_Delete(kind, refID, self\threads)
                self\deleteArmKind = ""
                self\deleteArmRefID = 0
                self\deleteArmAt = 0
            Else
                // Arm
                self\deleteArmKind = kind
                self\deleteArmRefID = refID
                self\deleteArmAt = MilliSecs()
                WriteLog(LoomLog, "Composer: delete armed for " + kind + "#" + Str(refID))
            EndIf
        EndIf
    End Method


    // -------------------------------------------------------------------------
    // drawDiscardButton -- reverts the current kind's in-memory state by
    // re-running its loader. Two-click arm/confirm like Delete. Only
    // rendered when the kind is dirty.
    // -------------------------------------------------------------------------
    Method drawDiscardButton(btnX%, btnY%, mx%, my%, clicked%, kind$)
        Local hovered% = (mx >= btnX And mx < btnX + CMP_DISCARD_BTN_W And my >= btnY And my < btnY + CMP_DISCARD_BTN_H)

        If self\discardArmKind <> "" And (MilliSecs() - self\discardArmAt) > CMP_DELETE_ARM_MS
            self\discardArmKind = ""
            self\discardArmAt = 0
        EndIf

        Local armed% = (self\discardArmKind = kind)

        If armed = True
            LoomFill(btnX, btnY, CMP_DISCARD_BTN_W, CMP_DISCARD_BTN_H, LOOM_WARNING_R, LOOM_WARNING_G, LOOM_WARNING_B)
            LoomBorder(btnX, btnY, CMP_DISCARD_BTN_W, CMP_DISCARD_BTN_H, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
            LoomText(btnX + 6, btnY + 4, "Confirm?", LOOM_INK_900_R, LOOM_INK_900_G, LOOM_INK_900_B)
        Else If hovered = True
            LoomFill(btnX, btnY, CMP_DISCARD_BTN_W, CMP_DISCARD_BTN_H, LOOM_STONE_700_R, LOOM_STONE_700_G, LOOM_STONE_700_B)
            LoomBorder(btnX, btnY, CMP_DISCARD_BTN_W, CMP_DISCARD_BTN_H, LOOM_WARNING_R, LOOM_WARNING_G, LOOM_WARNING_B)
            LoomText(btnX + 8, btnY + 4, "Discard", LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
        Else
            LoomFill(btnX, btnY, CMP_DISCARD_BTN_W, CMP_DISCARD_BTN_H, LOOM_STONE_800_R, LOOM_STONE_800_G, LOOM_STONE_800_B)
            LoomBorder(btnX, btnY, CMP_DISCARD_BTN_W, CMP_DISCARD_BTN_H, LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)
            LoomText(btnX + 8, btnY + 4, "Discard", LOOM_STONE_200_R, LOOM_STONE_200_G, LOOM_STONE_200_B)
        EndIf

        If hovered And clicked
            If armed = True
                Composer::discardKind(self, kind)
                self\discardArmKind = ""
                self\discardArmAt = 0
            Else
                self\discardArmKind = kind
                self\discardArmAt = MilliSecs()
                WriteLog(LoomLog, "Composer: discard armed for " + kind)
            EndIf
        EndIf
    End Method


    // -------------------------------------------------------------------------
    // discardKind -- reload the kind's in-memory state from disk, drop any
    // dirty flag. Uses GUE's existing Load* functions; because Load* New's
    // fresh Type instances and the in-memory state already has stale ones,
    // we have to free the old instances first.
    //
    // For Spell/Item/Actor: walk the *List arrays, Delete every non-Null
    // slot, then re-run LoadX which re-populates.
    // For AnimSet: walk AnimList[0..999], Delete each, re-run LoadAnimSets.
    // For Factions: just re-run LoadFactions -- it overwrites FactionNames$
    // in place.
    // For Zone (the focused one only -- per-zone .dat): re-run
    // ServerLoadArea on the original name. Tricky because the focused
    // handle becomes stale; close the composer first.
    // -------------------------------------------------------------------------
    Method discardKind(kind$)
        // Any discard reloads the whole kind from disk -- the cache
        // can't possibly be accurate after that. Invalidate up front so
        // every branch below benefits without per-branch repeats.
        WorldCache_Invalidate()

        If kind = "spell"
            Composer::freeAllSpells(self)
            LoadSpells("Data\Server Data\Spells.dat")
            SpellsSaved = True
            WriteLog(LoomLog, "Composer: discarded -- reloaded Spells.dat")
            Composer::reFocusOrClose(self, kind)
            Return
        EndIf
        If kind = "item"
            Composer::freeAllItems(self)
            LoadItems("Data\Server Data\Items.dat")
            ItemsSaved = True
            WriteLog(LoomLog, "Composer: discarded -- reloaded Items.dat")
            Composer::reFocusOrClose(self, kind)
            Return
        EndIf
        If kind = "actor"
            Composer::freeAllActors(self)
            LoadActors("Data\Server Data\Actors.dat")
            ActorsSaved = True
            WriteLog(LoomLog, "Composer: discarded -- reloaded Actors.dat")
            Composer::reFocusOrClose(self, kind)
            Return
        EndIf
        If kind = "animset"
            Composer::freeAllAnimSets(self)
            LoadAnimSets("Data\Game Data\Animations.dat")
            AnimsSaved = True
            WriteLog(LoomLog, "Composer: discarded -- reloaded Animations.dat")
            Composer::reFocusOrClose(self, kind)
            Return
        EndIf
        If kind = "faction"
            // LoadFactions overwrites FactionNames$ in-place; no free needed.
            LoadFactions("Data\Server Data\Factions.dat")
            FactionsSaved = True
            WriteLog(LoomLog, "Composer: discarded -- reloaded Factions.dat")
            Composer::reFocusOrClose(self, kind)
            Return
        EndIf
        If kind = "zone"
            // Reload only the focused zone's .dat. Its handle becomes
            // stale after ServerUnloadArea, so we capture the name first,
            // close the composer, free, and reload.
            Local Ar.Area = Object.Area(self\threads\focusID)
            If Ar = Null
                WriteLog(LoomLog, "Composer: discard zone -- stale handle, no-op")
                Return
            EndIf
            Local zoneName$ = Ar\Name$
            Threads::focus(self\threads, "", 0)
            Threads::clearStack(self\threads)
            ServerUnloadArea(Ar)
            ServerLoadArea(zoneName)
            ZoneSaved = True
            WriteLog(LoomLog, "Composer: discarded -- reloaded zone " + zoneName)
            Return
        EndIf
        WriteLog(LoomLog, "Composer: discardKind -- no handler for " + kind)
    End Method


    // -------------------------------------------------------------------------
    // Free-all helpers -- pre-discard cleanup. The Load* functions assume
    // they're populating a clean slate; not freeing first would leak the
    // current set into the type pool. Delegate to non-Strict helpers in
    // the data modules where possible (Dim-write trap means Loom can't
    // null array slots from Strict).
    // -------------------------------------------------------------------------
    Method freeAllSpells()
        Local id% = 0
        For id = 0 To 65534
            DeleteSpellTemplate(id)
        Next
    End Method

    Method freeAllItems()
        Local id% = 0
        For id = 0 To 65534
            DeleteItemTemplate(id)
        Next
    End Method

    Method freeAllActors()
        Local id% = 0
        For id = 0 To 65535
            DeleteActorTemplate(id)
        Next
    End Method

    Method freeAllAnimSets()
        Local id% = 0
        For id = 0 To 999
            DeleteAnimSetTemplate(id)
        Next
    End Method


    // -------------------------------------------------------------------------
    // reFocusOrClose -- after a discard reload, the focused refID may no
    // longer exist (e.g. you deleted a spell, then discarded -- the spell
    // is back but the ID may match a different newly-loaded spell, or none).
    // Safe behavior: if the focused entity still resolves, keep focus; else
    // close the composer back to the browser.
    // -------------------------------------------------------------------------
    Method reFocusOrClose(kind$)
        Local nm$ = Threads::lookupName(self\threads, kind, self\threads\focusID)
        If nm = ""
            Threads::focus(self\threads, "", 0)
            Threads::clearStack(self\threads)
        EndIf
    End Method


    // -------------------------------------------------------------------------
    // drawSaveButton -- top-right Save affordance, only painted by
    // renderAndUpdate when the focused kind is dirty.
    // -------------------------------------------------------------------------
    Method drawSaveButton(btnX%, btnY%, mx%, my%, clicked%, kind$)
        Local hovered% = (mx >= btnX And mx < btnX + CMP_SAVE_BTN_W And my >= btnY And my < btnY + CMP_SAVE_BTN_H)

        If hovered = True
            LoomFill(btnX, btnY, CMP_SAVE_BTN_W, CMP_SAVE_BTN_H, LOOM_ARCANE_700_R, LOOM_ARCANE_700_G, LOOM_ARCANE_700_B)
            LoomBorder(btnX, btnY, CMP_SAVE_BTN_W, CMP_SAVE_BTN_H, LOOM_ARCANE_500_R, LOOM_ARCANE_500_G, LOOM_ARCANE_500_B)
        Else
            LoomFill(btnX, btnY, CMP_SAVE_BTN_W, CMP_SAVE_BTN_H, LOOM_STONE_800_R, LOOM_STONE_800_G, LOOM_STONE_800_B)
            LoomBorder(btnX, btnY, CMP_SAVE_BTN_W, CMP_SAVE_BTN_H, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        EndIf
        LoomText(btnX + 18, btnY + 4, "Save", LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)

        If hovered And clicked
            // Commit any in-progress edit first so the buffer is written
            // before serialization.
            If self\editKind <> "" Then Composer::commitEdit(self)
            Composer::commitSaveForKind(self, kind)
        EndIf
    End Method


    // label : thread chip row. Returns the next Y. ORs into self\chipHit
    // when the chip is clicked so renderAndUpdate can surface it.
    //
    // editFieldId$: when non-empty + right-click consumed, open the palette
    // as a picker targeting (self\threads\focusKind, self\threads\focusID,
    // editFieldId). When empty (back-reference chips, e.g. faction members
    // or animset users), right-click is ignored -- there's no field on the
    // current focus to write the chosen entity into.
    Method chipRow%(panelX%, panelW%, rowY%, label$, kind$, refID%, mx%, my%, clicked%, rightClicked%, editFieldId$)
        LoomText(panelX + CMP_PAD, rowY + 4, label, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

        Local chipX% = panelX + CMP_PAD + 120
        Local chipW% = panelW - CMP_PAD * 2 - 120
        Local code% = Threads::renderChip(self\threads, chipX, rowY, chipW, CMP_CHIP_H, kind, refID, mx, my, clicked, rightClicked)
        If code = 1 Then self\chipHit = True
        If code = 2 And editFieldId <> "" And self\palette <> Null
            Palette::openAsPicker(self\palette, kind, self\threads\focusKind, self\threads\focusID, editFieldId)
        EndIf

        Return rowY + CMP_CHIP_H + 4
    End Method


    // Section header -- 3-line brass ornament rule + display-font label.
    // Returns the next Y. The triple rule mirrors the brand strip's
    // separator and the card-top accent so the visual rhythm is
    // consistent across surfaces.
    Method sectionHeader%(panelX%, panelW%, rowY%, title$)
        LoomHRule(panelX + CMP_PAD,     rowY + 4, panelW - CMP_PAD * 2, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        LoomHRule(panelX + CMP_PAD,     rowY + 5, panelW - CMP_PAD * 2, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomHRule(panelX + CMP_PAD,     rowY + 6, panelW - CMP_PAD * 2, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        LoomTheme_UseDisplay()
        LoomText(panelX + CMP_PAD,      rowY + 10, title, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomTheme_UseBody()
        Return rowY + 34
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

    Method renderActor(panelX%, bodyY%, panelW%, bodyH%, mx%, my%, clicked%, rightClicked%)
        Local refID% = self\threads\focusID
        If refID < 0 Or refID > 65535 Then Return
        Local A.Actor = ActorList(refID)
        If A = Null Then Return

        Local y% = bodyY
        y = Composer::row(self, panelX, panelW, y, "ID",            Str(A\ID))
        y = Composer::editableRow(self, panelX, panelW, y,    "Race",          "actor", A\ID, "race",          A\Race$,        mx, my, clicked)
        y = Composer::editableRow(self, panelX, panelW, y,    "Class",         "actor", A\ID, "class",         A\Class$,       mx, my, clicked)
        y = Composer::editableIntRow(self, panelX, panelW, y, "Aggressiveness", "actor", A\ID, "aggressiveness", A\Aggressiveness, mx, my, clicked)
        y = Composer::editableIntRow(self, panelX, panelW, y, "Agg range",     "actor", A\ID, "agg_range",     A\AggressiveRange, mx, my, clicked)
        y = Composer::editableIntRow(self, panelX, panelW, y, "Genders",       "actor", A\ID, "genders",       A\Genders,      mx, my, clicked)
        y = Composer::toggleRow(self,    panelX, panelW, y, "Playable",      "actor", A\ID, "playable",      A\Playable,     mx, my, clicked)
        y = Composer::toggleRow(self,    panelX, panelW, y, "Rideable",      "actor", A\ID, "rideable",      A\Rideable,     mx, my, clicked)
        y = Composer::editableIntRow(self, panelX, panelW, y, "XP multiplier", "actor", A\ID, "xpmult",        A\XPMultiplier, mx, my, clicked)
        y = Composer::editableFloatRow(self, panelX, panelW, y, "Scale",       "actor", A\ID, "scale",         A\Scale#,       mx, my, clicked)

        // Description -- a long string; show as editable text field. Word
        // wrap is a future enhancement.
        y = Composer::editableRow(self, panelX, panelW, y, "Description", "actor", A\ID, "description", A\Description$, mx, my, clicked)

        y = Composer::sectionHeader(self, panelX, panelW, y, "Threads")

        // Editable ref chips: right-click opens the palette as a picker
        // filtered to the chip's kind; selection writes the new refID
        // into the named field via Composer::writeField.
        y = Composer::chipRow(self, panelX, panelW, y, "Faction",    "faction", A\DefaultFaction, mx, my, clicked, rightClicked, "default_faction")
        y = Composer::chipRow(self, panelX, panelW, y, "M anim set", "animset", A\MAnimationSet,  mx, my, clicked, rightClicked, "manim_set")
        y = Composer::chipRow(self, panelX, panelW, y, "F anim set", "animset", A\FAnimationSet,  mx, my, clicked, rightClicked, "fanim_set")
    End Method


    Method renderItem(panelX%, bodyY%, panelW%, bodyH%, mx%, my%, clicked%)
        Local refID% = self\threads\focusID
        If refID < 0 Or refID > 65534 Then Return
        Local It.Item = ItemList(refID)
        If It = Null Then Return

        Local y% = bodyY
        y = Composer::row(self, panelX, panelW, y, "ID",        Str(It\ID))
        y = Composer::editableRow(self, panelX, panelW, y, "Name", "item", It\ID, "name", It\Name$, mx, my, clicked)
        y = Composer::row(self, panelX, panelW, y, "Type",      Composer::itemTypeLabel(self, It\ItemType))
        y = Composer::row(self, panelX, panelW, y, "Slot",      Str(It\SlotType))
        y = Composer::editableIntRow(self, panelX, panelW, y, "Value", "item", It\ID, "value", It\Value, mx, my, clicked)
        y = Composer::editableIntRow(self, panelX, panelW, y, "Mass",  "item", It\ID, "mass",  It\Mass,  mx, my, clicked)
        y = Composer::toggleRow(self, panelX, panelW, y, "Stackable", "item", It\ID, "stackable", It\Stackable,   mx, my, clicked)
        y = Composer::toggleRow(self, panelX, panelW, y, "Breakable", "item", It\ID, "breakable", It\TakesDamage, mx, my, clicked)

        // Weapon-specific
        If It\ItemType = 1
            y = Composer::sectionHeader(self, panelX, panelW, y, "Weapon")
            y = Composer::editableIntRow(self, panelX, panelW, y, "Damage",      "item", It\ID, "weapon_damage", It\WeaponDamage, mx, my, clicked)
            y = Composer::row(self, panelX, panelW, y, "Weapon type", Str(It\WeaponType))
            y = Composer::editableFloatRow(self, panelX, panelW, y, "Range",     "item", It\ID, "range",         It\Range#,       mx, my, clicked)
        EndIf

        // Armour-specific
        If It\ItemType = 2
            y = Composer::sectionHeader(self, panelX, panelW, y, "Armour")
            y = Composer::editableIntRow(self, panelX, panelW, y, "Armour level", "item", It\ID, "armour_level", It\ArmourLevel, mx, my, clicked)
        EndIf

        // Restrictions -- always editable (typing into an empty field is how
        // a restriction is added in the first place).
        y = Composer::sectionHeader(self, panelX, panelW, y, "Restricted to")
        y = Composer::editableRow(self, panelX, panelW, y, "Race",  "item", It\ID, "race",  It\ExclusiveRace$,  mx, my, clicked)
        y = Composer::editableRow(self, panelX, panelW, y, "Class", "item", It\ID, "class", It\ExclusiveClass$, mx, my, clicked)

        // Script -- always editable
        y = Composer::sectionHeader(self, panelX, panelW, y, "Script")
        y = Composer::editableRow(self, panelX, panelW, y, "Bound",  "item", It\ID, "script",  It\Script$,  mx, my, clicked)
        y = Composer::editableRow(self, panelX, panelW, y, "Method", "item", It\ID, "smethod", It\SMethod$, mx, my, clicked)
    End Method


    Method renderSpell(panelX%, bodyY%, panelW%, bodyH%, mx%, my%, clicked%)
        Local refID% = self\threads\focusID
        If refID < 0 Or refID > 65534 Then Return
        Local S.Spell = SpellsList(refID)
        If S = Null Then Return

        Local y% = bodyY
        y = Composer::row(self, panelX, panelW, y, "ID",          Str(S\ID))
        y = Composer::editableRow(self,    panelX, panelW, y, "Name",     "spell", S\ID, "name",        S\Name$,        mx, my, clicked)
        y = Composer::editableIntRow(self, panelX, panelW, y, "Recharge (ms)", "spell", S\ID, "recharge_ms", S\RechargeTime, mx, my, clicked)

        y = Composer::sectionHeader(self, panelX, panelW, y, "Description")
        y = Composer::editableRow(self, panelX, panelW, y, "Text", "spell", S\ID, "description", S\Description$, mx, my, clicked)

        y = Composer::sectionHeader(self, panelX, panelW, y, "Restricted to")
        y = Composer::editableRow(self, panelX, panelW, y, "Race",  "spell", S\ID, "race",  S\ExclusiveRace$,  mx, my, clicked)
        y = Composer::editableRow(self, panelX, panelW, y, "Class", "spell", S\ID, "class", S\ExclusiveClass$, mx, my, clicked)

        y = Composer::sectionHeader(self, panelX, panelW, y, "Script")
        y = Composer::editableRow(self, panelX, panelW, y, "Bound",  "spell", S\ID, "script",  S\Script$,  mx, my, clicked)
        y = Composer::editableRow(self, panelX, panelW, y, "Method", "spell", S\ID, "smethod", S\SMethod$, mx, my, clicked)
    End Method


    Method renderZone(panelX%, bodyY%, panelW%, bodyH%, mx%, my%, clicked%, rightClicked%)
        Local Ar.Area = Object.Area(self\threads\focusID)
        If Ar = Null Then Return
        Local h% = Handle(Ar)

        Local y% = bodyY
        y = Composer::editableRow(self,    panelX, panelW, y, "Name",     "zone", h, "name",     Ar\Name$,    mx, my, clicked)
        y = Composer::toggleRow(self,      panelX, panelW, y, "Outdoors", "zone", h, "outdoors", Ar\Outdoors, mx, my, clicked)
        y = Composer::toggleRow(self,      panelX, panelW, y, "PvP",      "zone", h, "pvp",      Ar\PvP,      mx, my, clicked)
        y = Composer::editableIntRow(self, panelX, panelW, y, "Gravity",  "zone", h, "gravity",  Ar\Gravity,  mx, my, clicked)

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

        // Scripts -- always editable
        y = Composer::sectionHeader(self, panelX, panelW, y, "Scripts")
        y = Composer::editableRow(self, panelX, panelW, y, "Entry", "zone", h, "entry_script", Ar\EntryScript$, mx, my, clicked)
        y = Composer::editableRow(self, panelX, panelW, y, "Exit",  "zone", h, "exit_script",  Ar\ExitScript$,  mx, my, clicked)

        // Portal links -- one chip per portal whose target resolves to a zone
        // we know about. The most-useful thread set zones can offer.
        If portals > 0
            y = Composer::sectionHeader(self, panelX, panelW, y, "Portal links")
            Local p% = 0
            For p = 0 To 99
                If Ar\PortalName$[p] <> "" And y < bodyY + bodyH - CMP_CHIP_H - 24
                    Local targetHandle% = Composer::findZoneByName(self, Ar\PortalLinkArea$[p])
                    If targetHandle <> 0
                        // Portal-target chips: editable via right-click. fieldId
                        // encodes the portal index ("portal_<i>") so writeField
                        // can route to the right slot. Picker writes the
                        // target zone's NAME (not handle) because the wire
                        // format stores by string.
                        y = Composer::chipRow(self, panelX, panelW, y, Ar\PortalName$[p], "zone", targetHandle, mx, my, clicked, rightClicked, "portal_" + Str(p))
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


    Method renderFaction(panelX%, bodyY%, panelW%, bodyH%, mx%, my%, clicked%, rightClicked%)
        Local idx% = self\threads\focusID
        If idx < 0 Or idx > 99 Then Return

        Local y% = bodyY
        y = Composer::editableRow(self, panelX, panelW, y, "Name",  "faction", idx, "name", FactionNames$(idx), mx, my, clicked)
        y = Composer::row(self, panelX, panelW, y, "Index", Str(idx))

        // Members -- every actor whose DefaultFaction matches. Each renders
        // as an actor chip. Capped to whatever fits in the panel body.
        y = Composer::sectionHeader(self, panelX, panelW, y, "Members")

        Local memberCount% = 0
        For Ac.Actor = Each Actor
            If Ac\DefaultFaction = idx And y < bodyY + bodyH - CMP_CHIP_H - 24
                // Members are a back-reference; no field on the focused
                // faction to edit via this chip, so editFieldId = "".
                y = Composer::chipRow(self, panelX, panelW, y, "", "actor", Ac\ID, mx, my, clicked, rightClicked, "")
                memberCount = memberCount + 1
            EndIf
        Next

        If memberCount = 0
            LoomText(panelX + CMP_PAD, y + 4, "(no members)", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)
        EndIf
    End Method


    Method renderAnimSet(panelX%, bodyY%, panelW%, bodyH%, mx%, my%, clicked%, rightClicked%)
        Local targetID% = self\threads\focusID

        // AnimSet is iterated, not indexed -- walk to find.
        Local A.AnimSet = Null
        For As.AnimSet = Each AnimSet
            If As\ID = targetID Then A = As : Exit
        Next
        If A = Null Then Return

        Local y% = bodyY
        y = Composer::editableRow(self, panelX, panelW, y, "Name", "animset", A\ID, "name", A\Name$, mx, my, clicked)
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
                // Back-reference (actors using this anim set); no field
                // on the focused animset to edit via this chip.
                y = Composer::chipRow(self, panelX, panelW, y, "", "actor", Ac\ID, mx, my, clicked, rightClicked, "")
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
