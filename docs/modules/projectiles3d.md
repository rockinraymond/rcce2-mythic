<!-- body { color:black background-color:white } a:link{ color:#0070FF } a:visited{ color:#0070FF } --> RealmCrafter: Community Edition Documentation

**Projectiles3D.bb**

Client-side projectile rendering. The whole module is three functions plus one Type — total source is ~107 lines. Owns the `ProjectileInstance` Type, the per-tick `UpdateProjectiles` mover that walks every live projectile and frees them on impact, and the `CreateProjectile` / `FreeProjectileInstance` lifecycle pair.

The server side of projectile gameplay (damage application, hit registration, target validation) lives in [`Projectiles.bb`](projectiles.md). This module is *only* the visual representation on the client.

## Conceptual overview

### `ProjectileInstance` Type

```basic
Type ProjectileInstance
    Field Target.ActorInstance       ; homing target (Null = fire-and-forget at TargetX/Y/Z)
    Field TargetX#, TargetY#, TargetZ#  ; resolved coordinate target when Target is Null
    Field EN, EmitterEN1, EmitterEN2 ; main mesh entity + two RottParticles emitters
    Field TexID1, TexID2             ; texture IDs used by the emitters (for unload bookkeeping)
    Field Speed#
End Type
```

The Type is allocated by `CreateProjectile` and `Delete`d by `FreeProjectileInstance`. There is no `Dim` array of projectiles — the global `For Each ProjectileInstance` walk in `UpdateProjectiles` is the only enumeration path.

### Homing vs. fire-and-forget

The `Homing` argument to `CreateProjectile` switches between two modes:

- **`Homing = True`**: `P\Target` is set to the target `ActorInstance`. Each `UpdateProjectiles` tick re-reads the target's current `CollisionEN` position. Tracks moving targets.
- **`Homing = False`**: `P\TargetX/Y/Z` are sampled **once** from the target's `CollisionEN` at creation, and `P\Target` stays `Null`. The projectile flies to those frozen coordinates regardless of target movement.

The destroy check is `EntityDistance#(P\EN, GPP) < 2.0` (where `GPP` is the global position pivot positioned at the current target each tick) — a 2-unit-radius proximity. Either mode lands within that radius; homing just re-aims toward a moving target.

### Per-tick walk: after-cursor pattern

```basic
Function UpdateProjectiles()
    Local P.ProjectileInstance = First ProjectileInstance
    Local PNext.ProjectileInstance = Null
    While P <> Null
        PNext = After P             ; capture next BEFORE the Delete branch
        ; ... move, retarget, then maybe FreeProjectileInstance(P) ...
        P = PNext
    Wend
End Function
```

The audit-comment block at [`Projectiles3D.bb:62-66`](../../src/Modules/Projectiles3D.bb#L62) records why this shape is mandatory: `FreeProjectileInstance(P)` calls `Delete(P)`, and a naive `For Each ... Next` cursor would then dereference the freed object's next pointer on the following iteration step. The capture-`After`-before-`Delete` shape is one of the three established iterator-during-iteration fixes (CLAUDE.md → "Iterator-during-iteration hazards"). The trigger case is two projectiles landing in the same frame.

### Mesh + emitter binding

`CreateProjectile` allocates resources in three stages:

1. **Main mesh** (`P\EN`): looked up via `GetMesh(MeshID)` if `MeshID > -1 And MeshID < 65535`; scaled with `LoadedMeshScales#(MeshID)`. If the lookup fails (template missing or out-of-range ID), falls back to `CreatePivot()` so the projectile is still a positionable transform — emitters and the EntityDistance check still work on an invisible pivot.
2. **Emitter 1** (`P\EmitterEN1`): created via `RP_LoadEmitterConfig("Data\Emitter Configs\<name>.rpc", Tex, Cam)` + `RP_CreateEmitter(Config)`, parented to `P\EN`. The texture ID is remembered in `P\TexID1` for later `UnloadTexture`.
3. **Emitter 2** (`P\EmitterEN2`): same shape as emitter 1.

Both emitters are optional — empty `Emitter1$` / `Emitter2$` strings skip the allocation. The texture lookup goes through `GetTexture(TexID)`; a failed lookup also skips the emitter (no fallback).

`FreeProjectileInstance` undoes all three in reverse: `UnloadTexture` for each `TexID*` that's `> -1`, `RP_KillEmitter` for each emitter that's `<> 0` (re-parented to root before kill so the emitter doesn't get yanked with the parent mesh), `FreeEntity(P\EN)`, `Delete(P)`.

### Globals it reads

The module doesn't define globals itself but reads three from elsewhere:

- **`Cam`** — the world camera handle (defined in [`Environment3D.bb`](environment3d.md)). Passed to `RP_LoadEmitterConfig` as the billboard camera.
- **`GPP`** — the global position pivot, reused by `UpdateProjectiles` to position the target coordinate so `EntityDistance` can be called.
- **`Delta#`** — the frame delta, used to scale `MoveEntity(P\EN, 0, 0, P\Speed# * Delta#)` for framerate-independent movement.
- **`LoadedMeshScales#(MeshID)`** — per-template scale factor, sourced from the mesh registry.

## Conventions for new code touching this module

- **All per-frame walks over `ProjectileInstance` must use the after-cursor pattern** (`First` + `After` + `PNext` capture). Free-current-during-walk is the most common operation; the `For Each` iterator can't survive it. The single existing site at `UpdateProjectiles` is the canonical example.
- **Texture and emitter handles are owned by the projectile.** Never share a `TexID` across projectiles without a refcount — `FreeProjectileInstance` will `UnloadTexture` the first projectile's texture and the second projectile will render with a stale handle.
- **`P\EN = 0` is impossible** — if the mesh lookup fails, `CreatePivot()` always succeeds (returns a non-zero handle). No null-deref guard needed downstream.
- **Stale `P\Target` from a freed actor** — `UpdateProjectiles` reads `P\Target\CollisionEN` without a `Null` check. If `FreeActorInstance` runs while a projectile is in flight toward that actor, the next tick dereferences a freed handle. There is no current cleanup hook; a follow-up could either iterate live projectiles in `FreeActorInstance` and clear `Target`, or guard the deref with `Object.ActorInstance(Handle(P\Target)) <> Null` per CLAUDE.md → "Handle-lookup Null discipline".

## Related modules

- [`Projectiles.bb`](projectiles.md) — server-side projectile logic (damage, target validation, hit packets). The client `CreateProjectile` is driven by packets emitted from here.
- [`RottParticles.bb`](rottparticles.md) — supplies `RP_LoadEmitterConfig` / `RP_CreateEmitter` / `RP_KillEmitter`. The emitter substrate.
- [`Environment3D.bb`](environment3d.md) — owns `Cam` (the world camera) and the entity-management primitives.
- [`Spells.bb`](spells.md) — caller (server-side spell-cast packets trigger client-side `CreateProjectile`).
- [`Actors3D.bb`](actors3d.md) — owns `ActorInstance\CollisionEN`, the source/target collision entity that the projectile homes on.

## See also

- CLAUDE.md → "Iterator-during-iteration hazards" — the after-cursor walk pattern. `UpdateProjectiles` is one of the canonical examples cited there.

* * *

The full source at [`src/Modules/Projectiles3D.bb`](../../src/Modules/Projectiles3D.bb) is short enough that a function-by-function reference adds little. The three public functions are `CreateProjectile`, `UpdateProjectiles`, and `FreeProjectileInstance`.
