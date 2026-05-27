Strict

// =============================================================================
// Loom/EntityFactory.bb -- create new entities from the Loom UI
// =============================================================================
//
// Wraps GUE's existing Create* constructors so the Browser's "+ New" button
// can spawn a fresh actor / item / spell / zone / faction / anim set with
// sensible defaults, focus it for editing, and mark its kind dirty so the
// Save button appears.
//
// Why a free-function module (and not a Type with Methods like the other
// Loom UI modules)? EntityFactory holds no state -- it's pure dispatch
// from (kind, threads*) -> "create + focus + dirty". Stateless helpers are
// the project's canonical shape for "free functions are fine" per the
// BlitzForge skill (see Theme.bb for the established pattern).
//
// Constructors used (from GUE's data modules):
//   actor    -> CreateActor.Actor()       (Actors.bb:541)
//   item     -> CreateItem.Item()         (Items.bb:193)
//   spell    -> CreateSpell.Spell()       (Spells.bb:24)
//   zone     -> ServerCreateArea.Area()   (ServerAreas.bb:107) + UniqueZoneName
//   animset  -> CreateAnimSet()           (Animations.bb:69)   (returns slot ID)
//   faction  -> first empty FactionNames$ slot + SetFactionName (Actors.bb)
//
// Per-kind dirty-flag globals are the same set Composer.bb writes to; the
// Loom.bb declarations re-export the SaveX globals so we can flip them
// here without an extra getter/setter pair.


// =============================================================================
// EntityFactory_Create -- dispatch on kind, return True if an entity was
// created. Out-params: focusKind / focusID get the new entity's identity so
// the caller can Threads::focus on it.
//
// BlitzForge doesn't have true out-params; the caller passes a Threads
// pointer and we call Threads::focus directly. That's also the right
// behavior -- creation should always focus the new entity for editing.
// =============================================================================
Function EntityFactory_Create%(kind$, threads.Threads)
    If kind = "actor"   Then Return EntityFactory_CreateActor(threads)
    If kind = "item"    Then Return EntityFactory_CreateItem(threads)
    If kind = "spell"   Then Return EntityFactory_CreateSpell(threads)
    If kind = "zone"    Then Return EntityFactory_CreateZone(threads)
    If kind = "faction" Then Return EntityFactory_CreateFaction(threads)
    If kind = "animset" Then Return EntityFactory_CreateAnimSet(threads)

    WriteLog(LoomLog, "EntityFactory: unknown kind '" + kind + "'")
    Return False
End Function


// -----------------------------------------------------------------------------
// Per-kind creators. Each returns True on success, False on cap (e.g. no
// free slot in a fixed-size array). On success: focuses the new entity,
// flips the kind's *Saved global to False (dirty), logs the new ID.
// -----------------------------------------------------------------------------

Function EntityFactory_CreateActor%(threads.Threads)
    Local A.Actor = CreateActor()
    If A = Null
        WriteLog(LoomLog, "EntityFactory: CreateActor returned Null (ActorList full?)")
        Return False
    EndIf
    A\Race$ = "New"
    A\Class$ = "Actor"
    A\XPMultiplier = 1
    A\Aggressiveness = 0
    A\Genders = 0
    Threads::focus(threads, "actor", A\ID)
    ActorsSaved = False
    WriteLog(LoomLog, "EntityFactory: created actor #" + Str(A\ID))
    Return True
End Function


Function EntityFactory_CreateItem%(threads.Threads)
    Local I.Item = CreateItem()
    If I = Null
        WriteLog(LoomLog, "EntityFactory: CreateItem returned Null (ItemList full?)")
        Return False
    EndIf
    I\Name$ = "New Item"
    Threads::focus(threads, "item", I\ID)
    ItemsSaved = False
    WriteLog(LoomLog, "EntityFactory: created item #" + Str(I\ID))
    Return True
End Function


Function EntityFactory_CreateSpell%(threads.Threads)
    Local S.Spell = CreateSpell()
    If S = Null
        WriteLog(LoomLog, "EntityFactory: CreateSpell returned Null (SpellsList full?)")
        Return False
    EndIf
    // CreateSpell defaults Name to "New ability" -- keep as-is for muscle-
    // memory parity with GUE's behavior.
    Threads::focus(threads, "spell", S\ID)
    SpellsSaved = False
    WriteLog(LoomLog, "EntityFactory: created spell #" + Str(S\ID))
    Return True
End Function


Function EntityFactory_CreateZone%(threads.Threads)
    Local A.Area = ServerCreateArea()
    If A = Null
        WriteLog(LoomLog, "EntityFactory: ServerCreateArea returned Null")
        Return False
    EndIf
    A\Name$ = EntityFactory_UniqueZoneName$("New Zone")
    Threads::focus(threads, "zone", Handle(A))
    ZoneSaved = False
    WriteLog(LoomLog, "EntityFactory: created zone " + A\Name$)
    Return True
End Function


Function EntityFactory_CreateFaction%(threads.Threads)
    // Find first empty FactionNames$ slot. The array is Dim'd at 99 so
    // indices 0..99 = 100 slots total.
    Local slot% = -1
    Local i% = 0
    For i = 0 To 99
        If FactionNames$(i) = ""
            slot = i
            Exit
        EndIf
    Next
    If slot = -1
        WriteLog(LoomLog, "EntityFactory: faction roster full (100 slots used)")
        Return False
    EndIf

    // SetFactionName lives in Actors.bb (non-Strict) so this Strict file
    // can write the global without hitting the Dim-inside-Method trap.
    SetFactionName(slot, "New Faction")
    Threads::focus(threads, "faction", slot)
    FactionsSaved = False
    WriteLog(LoomLog, "EntityFactory: created faction slot " + Str(slot))
    Return True
End Function


Function EntityFactory_CreateAnimSet%(threads.Threads)
    // CreateAnimSet returns the assigned ID (or -1 on cap). The instance
    // is already inserted into AnimList by the constructor.
    Local newID% = CreateAnimSet()
    If newID = -1
        WriteLog(LoomLog, "EntityFactory: CreateAnimSet returned -1 (AnimList full)")
        Return False
    EndIf
    Threads::focus(threads, "animset", newID)
    AnimsSaved = False
    WriteLog(LoomLog, "EntityFactory: created animset #" + Str(newID))
    Return True
End Function


// -----------------------------------------------------------------------------
// EntityFactory_UniqueZoneName -- return a name with " 2" / " 3" / ...
// appended until no existing zone matches. ServerSaveArea uses Name$ as the
// filename, so a duplicate would silently overwrite an existing zone file.
// -----------------------------------------------------------------------------
Function EntityFactory_UniqueZoneName$(base$)
    If EntityFactory_ZoneNameExists%(base) = False Then Return base
    Local i% = 2
    While i < 1000
        Local candidate$ = base + " " + Str(i)
        If EntityFactory_ZoneNameExists%(candidate) = False Then Return candidate
        i = i + 1
    Wend
    Return base + " " + Str(MilliSecs())     // unreachable in practice
End Function


Function EntityFactory_ZoneNameExists%(name$)
    Local upr$ = Upper$(name)
    For A.Area = Each Area
        If Upper$(A\Name$) = upr Then Return True
    Next
    Return False
End Function
