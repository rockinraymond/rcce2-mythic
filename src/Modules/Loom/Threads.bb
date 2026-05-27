// =============================================================================
// Loom/Threads.bb -- focus + back-stack navigation + clickable thread chips
// =============================================================================
//
// The centerpiece of the Loom design: every reference between entities is
// rendered as a clickable chip. Clicking a chip jumps the focused entity
// from the source to the target and pushes the source onto a back stack;
// Esc pops the stack to walk back through the navigation chain.
//
// Hero flow this enables:
//   browse to "Goblin Shaman" -> composer paints the actor ->
//   click [◈ Forest Tribe] chip on its faction field ->
//   composer paints Forest Tribe with its member roster ->
//   click [◈ Goblin Scout] in that roster ->
//   composer paints that actor ->
//   Esc -> back to Forest Tribe ->
//   Esc -> back to Goblin Shaman.
//
// Entity-kind identifier convention (kept consistent across Browser /
// Composer / Threads / Loom.bb):
//
//   ""         no entity focused
//   "actor"    refID = Actor\ID (array index in ActorList)
//   "item"     refID = Item\ID  (array index in ItemList)
//   "spell"    refID = Spell\ID (array index in SpellsList)
//   "zone"     refID = Handle(Area)
//   "faction"  refID = FactionNames$ array index 0..99
//   "animset"  refID = AnimSet\ID
//
// Public API:
//   Threads_Init()
//     Initialize the back stack. Call once at startup.
//
//   Threads_Focus(kind$, refID)
//     Set the focus directly. Does NOT push the previous focus onto the
//     back stack. Use when entering composer from the browser, or when
//     restoring after a Threads_Back.
//
//   Threads_Jump(kind$, refID)
//     Push the current focus, then set the new focus. This is what every
//     thread-chip click does. No-op if jumping to the same entity that's
//     already focused.
//
//   Threads_Back() -> True if popped
//     Restore the most recently pushed focus. Returns False (and leaves
//     focus unchanged) when the back stack is empty.
//
//   Threads_ClearStack()
//     Drop the entire back stack. Call when leaving composer back to the
//     browser so a new browse session doesn't inherit stale history.
//
//   Threads_LookupName$(kind$, refID)
//     Resolve the entity to its display name. Returns "" if the reference
//     doesn't resolve (the entity was deleted) -- callers treat empty as
//     a broken-ref signal and render the chip with broken styling.
//
//   Threads_RenderChip(x, y, w, h, kind$, refID, mx, my, clicked) -> True if click consumed
//     Draws a chip rect with the kind icon + target name. Hit-tests the
//     mouse against the chip; if clicked, calls Threads_Jump internally
//     and returns True. The composer just lays out chips and watches the
//     return value to react when the user follows a thread.
// =============================================================================


// -----------------------------------------------------------------------------
// State -- focus + back stack
// -----------------------------------------------------------------------------
Global Loom_FocusKind$ = ""
Global Loom_FocusID    = 0

Type LoomFocusEntry
    Field Kind$
    Field RefID
End Type
Global Loom_BackStack.BBList = Null


// -----------------------------------------------------------------------------
// Chip layout constants
// -----------------------------------------------------------------------------
Const CHIP_PAD_X = 10
Const CHIP_ICON_W = 18


// =============================================================================
// Threads_Init
// =============================================================================
Function Threads_Init()
    Loom_BackStack = CreateList()
    Loom_FocusKind$ = ""
    Loom_FocusID = 0
End Function


// =============================================================================
// Threads_Focus -- direct set, no stack push
// =============================================================================
Function Threads_Focus(kind$, refID)
    Loom_FocusKind$ = kind$
    Loom_FocusID = refID
End Function


// =============================================================================
// Threads_Jump -- push current focus, set new
// =============================================================================
Function Threads_Jump(kind$, refID)
    // No-op on self-link
    If kind$ = Loom_FocusKind$ And refID = Loom_FocusID Then Return

    // Push current focus (if any) onto the back stack
    If Loom_FocusKind$ <> ""
        Local prev.LoomFocusEntry = New LoomFocusEntry()
        prev\Kind$ = Loom_FocusKind$
        prev\RefID = Loom_FocusID
        ListAdd(Loom_BackStack, prev)
    EndIf

    Loom_FocusKind$ = kind$
    Loom_FocusID = refID

    WriteLog(LoomLog, "Threads: jumped to " + kind$ + "#" + Str(refID) + " (back stack: " + Str(ListSize(Loom_BackStack)) + ")")
End Function


// =============================================================================
// Threads_Back -- pop and focus; returns True if popped
// =============================================================================
Function Threads_Back()
    Local n = ListSize(Loom_BackStack)
    If n = 0 Then Return False

    // Pop the last entry. The LoomFocusEntry instance lives on the heap;
    // ListRemove only drops the list's reference to it, leaving the instance
    // itself leaked. Without `EnableGC` at the top of this file, Blitz3D has
    // no reference counting -- a long Loom session with N back/forward
    // navigations leaks N LoomFocusEntry instances. Capture the fields we
    // need, drop from list, then `Delete` the instance explicitly.
    Local prev.LoomFocusEntry = ListAt(Loom_BackStack, n - 1)
    If prev = Null Then Return False
    Local kind$ = prev\Kind$
    Local refID = prev\RefID
    ListRemove(Loom_BackStack, n - 1)
    Delete prev

    Loom_FocusKind$ = kind$
    Loom_FocusID = refID

    WriteLog(LoomLog, "Threads: back to " + kind$ + "#" + Str(refID) + " (back stack: " + Str(ListSize(Loom_BackStack)) + ")")
    Return True
End Function


// =============================================================================
// Threads_ClearStack
// =============================================================================
Function Threads_ClearStack()
    If Loom_BackStack = Null Then Return
    // Same leak rationale as Threads_Back: ListClear only drops the list's
    // references; the LoomFocusEntry instances must be Deleted explicitly
    // (no EnableGC at file top, no auto-collection).
    Local entry.LoomFocusEntry
    For entry = Each LoomFocusEntry
        Delete entry
    Next
    ListClear(Loom_BackStack)
End Function


// =============================================================================
// Threads_LookupName$ -- resolve to display name. Returns "" if not found.
// =============================================================================
Function Threads_LookupName$(kind$, refID)
    If kind$ = "actor"
        If refID < 0 Or refID > 65535 Then Return ""
        Local Ac.Actor = ActorList(refID)
        If Ac = Null Then Return ""
        Return Ac\Race$ + " [" + Ac\Class$ + "]"
    EndIf

    If kind$ = "item"
        If refID < 0 Or refID > 65534 Then Return ""
        Local It.Item = ItemList(refID)
        If It = Null Then Return ""
        Return It\Name$
    EndIf

    If kind$ = "spell"
        If refID < 0 Or refID > 65534 Then Return ""
        Local Sp.Spell = SpellsList(refID)
        If Sp = Null Then Return ""
        Return Sp\Name$
    EndIf

    If kind$ = "zone"
        Local Ar.Area = Object.Area(refID)
        If Ar = Null Then Return ""
        Return Ar\Name$
    EndIf

    If kind$ = "faction"
        If refID < 0 Or refID > 99 Then Return ""
        Local n$ = FactionNames$(refID)
        If n$ = "" Then Return ""
        Return n$
    EndIf

    If kind$ = "animset"
        // AnimSet is iterated, not array-indexed -- walk to find by ID.
        For As.AnimSet = Each AnimSet
            If As\ID = refID Then Return As\Name$
        Next
        Return ""
    EndIf

    Return ""
End Function


// =============================================================================
// Threads_RenderChip -- draw a clickable chip, hit-test, jump on click.
//
// Returns True if the chip consumed a click this frame. The composer can
// ignore the return value if it doesn't need it -- the jump has already
// happened by then.
// =============================================================================
Function Threads_RenderChip(x, y, w, h, kind$, refID, mx, my, clicked)
    Local hovered = (mx >= x And mx < x + w And my >= y And my < y + h)

    Local name$ = Threads_LookupName$(kind$, refID)
    Local broken = (name$ = "")
    If broken = True Then name$ = "(broken " + kind$ + " #" + Str(refID) + ")"

    // Background
    If hovered = True And broken = False
        LoomFill(x, y, w, h, LOOM_ARCANE_900_R, LOOM_ARCANE_900_G, LOOM_ARCANE_900_B)
    Else
        LoomFill(x, y, w, h, LOOM_STONE_800_R, LOOM_STONE_800_G, LOOM_STONE_800_B)
    EndIf

    // Border -- broken refs get a danger-red border so they read as wrong
    If broken = True
        LoomBorder(x, y, w, h, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
    Else If hovered = True
        LoomBorder(x, y, w, h, LOOM_ARCANE_500_R, LOOM_ARCANE_500_G, LOOM_ARCANE_500_B)
        LoomBorder(x + 1, y + 1, w - 2, h - 2, LOOM_ARCANE_500_R, LOOM_ARCANE_500_G, LOOM_ARCANE_500_B)
    Else
        LoomBorder(x, y, w, h, LOOM_BRASS_700_R, LOOM_BRASS_700_G, LOOM_BRASS_700_B)
    EndIf

    // Kind icon (text glyph in brass, left-aligned)
    Local icon$ = Threads_KindGlyph$(kind$)
    LoomText(x + CHIP_PAD_X, y + 5, icon$, LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)

    // Name (parchment, dimmer if broken)
    If broken = True
        LoomText(x + CHIP_PAD_X + CHIP_ICON_W, y + 5, name$, LOOM_DANGER_R, LOOM_DANGER_G, LOOM_DANGER_B)
    Else
        LoomText(x + CHIP_PAD_X + CHIP_ICON_W, y + 5, name$, LOOM_PARCHMENT_100_R, LOOM_PARCHMENT_100_G, LOOM_PARCHMENT_100_B)
    EndIf

    // Right-side arrow indicating "jump"
    If broken = False
        LoomText(x + w - 16, y + 5, ">", LOOM_BRASS_500_R, LOOM_BRASS_500_G, LOOM_BRASS_500_B)
    EndIf

    // Click consumed?
    If hovered And clicked And broken = False
        Threads_Jump(kind$, refID)
        Return True
    EndIf
    Return False
End Function


// Short glyph used as the kind icon in chips and cards. Picked from common
// Unicode that the default Blitz font renders (ASCII fallbacks where it
// doesn't).
Function Threads_KindGlyph$(kind$)
    If kind$ = "actor"   Then Return "A"
    If kind$ = "item"    Then Return "I"
    If kind$ = "spell"   Then Return "S"
    If kind$ = "zone"    Then Return "Z"
    If kind$ = "faction" Then Return "F"
    If kind$ = "animset" Then Return "M"
    Return "?"
End Function
