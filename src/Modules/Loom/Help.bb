Strict

// =============================================================================
// Loom/Help.bb -- F1 keyboard shortcuts cheat sheet
// =============================================================================
//
// Loom's keybinding suite grew from "Esc + arrows" in the alpha to
// "Ctrl+K / Ctrl+H / Ctrl+R / Ctrl+S + arrows + Enter + right-click +
// the Esc cascade" in the beta. Without a discovery surface, users
// can't find them.
//
// F1 opens this modal showing every keybinding + mouse interaction in
// one table. Esc closes. The contents are static -- no per-session
// state, no scrolling needed at the current shortcut count.
//
// Architecture: Type with Methods (mirrors Timeline / Recents / BrokenRefs
// / Palette / ExitPrompt -- the five other modal surfaces).


Const HELP_MODAL_W   = 680
Const HELP_MODAL_H   = 520
Const HELP_PAD       = 16
Const HELP_HEADER_H  = 32
Const HELP_HINT_H    = 24
Const HELP_ROW_H     = 22
Const HELP_KEY_COL_W = 200


// =============================================================================
// Help -- F1 cheat sheet modal.
// =============================================================================
Type Help
    Field open%


    Method create.Help()
        self\open = False
        Return self
    End Method


    Method isOpen%()
        Return self\open
    End Method


    Method openModal()
        self\open = True
        FlushKeys
        WriteLog(LoomLog, "Help: open")
    End Method


    Method closeModal()
        self\open = False
        WriteLog(LoomLog, "Help: close")
    End Method


    Method renderAndUpdate%(sw%, sh%)
        If self\open = False Then Return False

        If KeyHit(1) Or KeyHit(59)   // Esc or F1 toggle-off
            Help::closeModal(self)
            Return True
        EndIf

        LoomFill(0, 0, sw, sh, LOOM_STONE_950_R, LOOM_STONE_950_G, LOOM_STONE_950_B)

        Local mx% = MouseX()
        Local my% = MouseY()
        Local clicked% = MouseHit(1)

        Local modalX% = (sw - HELP_MODAL_W) / 2
        Local modalY% = (sh - HELP_MODAL_H) / 3

        LoomFill(modalX, modalY, HELP_MODAL_W, HELP_MODAL_H, LOOM_STONE_850_R, LOOM_STONE_850_G, LOOM_STONE_850_B)
        LoomBorder(modalX, modalY, HELP_MODAL_W, HELP_MODAL_H, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomBorder(modalX + 1, modalY + 1, HELP_MODAL_W - 2, HELP_MODAL_H - 2, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        LoomFill(modalX, modalY, HELP_MODAL_W, 3, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

        // Header in display font
        LoomTheme_UseDisplay()
        LoomText(modalX + HELP_PAD, modalY + 6, "LOOM  |  KEYBINDINGS", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomTheme_UseBody()

        // Body -- rendered as a two-column table via per-row helpers so
        // the row layout stays consistent.
        Local rowY% = modalY + HELP_HEADER_H + 12

        rowY = Help::section(self, modalX, rowY, "Global")
        rowY = Help::row(self, modalX, rowY, "Ctrl+K",       "Command palette (find anywhere)")
        rowY = Help::row(self, modalX, rowY, "Ctrl+H",       "Session timeline (edit history + revert)")
        rowY = Help::row(self, modalX, rowY, "Ctrl+R",       "Recents (jump to recently-focused entity)")
        rowY = Help::row(self, modalX, rowY, "Ctrl+S",       "Save All (every dirty kind)")
        rowY = Help::row(self, modalX, rowY, "F1",           "This help screen")
        rowY = Help::row(self, modalX, rowY, "Esc",          "Pop / close / exit (priority chain)")
        rowY = rowY + 6

        rowY = Help::section(self, modalX, rowY, "Browser")
        rowY = Help::row(self, modalX, rowY, "Arrow keys",   "Move card selection cursor")
        rowY = Help::row(self, modalX, rowY, "Enter",        "Focus the selected card")
        rowY = Help::row(self, modalX, rowY, "Type letters", "Filter the current tab by name")
        rowY = Help::row(self, modalX, rowY, "Click tab",    "Switch category")
        rowY = Help::row(self, modalX, rowY, "Click + New",  "Create a fresh entity of the current kind")
        rowY = rowY + 6

        rowY = Help::section(self, modalX, rowY, "Composer (focused entity)")
        rowY = Help::row(self, modalX, rowY, "Click field",        "Begin editing (text / number)")
        rowY = Help::row(self, modalX, rowY, "Enter",              "Commit edit")
        rowY = Help::row(self, modalX, rowY, "Esc (during edit)",  "Cancel edit")
        rowY = Help::row(self, modalX, rowY, "Click toggle pill",  "Flip a bool field")
        rowY = Help::row(self, modalX, rowY, "Left-click chip",    "Jump to referenced entity")
        rowY = Help::row(self, modalX, rowY, "Right-click chip",   "Open palette as picker (swap referent)")
        rowY = Help::row(self, modalX, rowY, "Click Save / X / Discard", "Persist / delete (arm) / revert")
        rowY = rowY + 6

        rowY = Help::section(self, modalX, rowY, "Ribbon (top strip)")
        rowY = Help::row(self, modalX, rowY, "Click dirty badge",        "Save that kind")
        rowY = Help::row(self, modalX, rowY, "Click broken-ref count",   "Open the broken-ref finder")

        // Footer hint
        Local hy% = modalY + HELP_MODAL_H - HELP_HINT_H - 4
        LoomHRule(modalX + HELP_PAD, hy - 2, HELP_MODAL_W - HELP_PAD * 2, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        LoomText(modalX + HELP_PAD, hy + 4, "Esc or F1 to close", LOOM_STONE_300_R, LOOM_STONE_300_G, LOOM_STONE_300_B)

        // Click-outside-modal closes
        If clicked = True
            If mx < modalX Or mx >= modalX + HELP_MODAL_W Or my < modalY Or my >= modalY + HELP_MODAL_H
                Help::closeModal(self)
            EndIf
        EndIf

        Return True
    End Method


    // -------------------------------------------------------------------------
    // section -- brass-underlined section header. Returns next y.
    // -------------------------------------------------------------------------
    Method section%(modalX%, y%, title$)
        LoomText(modalX + HELP_PAD, y, title, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
        LoomHRule(modalX + HELP_PAD, y + 18, HELP_MODAL_W - HELP_PAD * 2, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
        Return y + 24
    End Method


    // -------------------------------------------------------------------------
    // row -- two-column "key : description" row. Returns next y.
    // -------------------------------------------------------------------------
    Method row%(modalX%, y%, keyLabel$, desc$)
        LoomText(modalX + HELP_PAD,                  y, keyLabel, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
        LoomText(modalX + HELP_PAD + HELP_KEY_COL_W, y, desc,     LOOM_STONE_200_R, LOOM_STONE_200_G, LOOM_STONE_200_B)
        Return y + HELP_ROW_H
    End Method
End Type
