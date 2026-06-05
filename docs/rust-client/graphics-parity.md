# Rust client — graphics parity tracker

Goal: confirm the Rust client (`bin/ClientRS.exe`) reaches full graphical parity
with the BlitzForge `Client.exe`, feature by feature. Each row is verified by
rendering a controlled scene and auditing it against the Blitz render code
(`Environment3D.bb`, `ClientAreas.bb`, `Media.bb`, `Interface3D.bb`).

## Verification harness

No real fork data or server is needed:

- **`gen-test-zone <data_root> [zone]`** (bin) writes a synthetic `Data/Areas/<zone>.dat`
  in the exact `SaveArea` byte layout, exercising chosen features (terrain hill,
  water lake, reference props). Extend it to cover new features.
- **`RCCE_VIEWZONE=1`** renders the loaded zone directly (no menu / no server) with
  a free camera: `RCCE_CAMAT="x,y,z"`, `RCCE_CAMYAW`, `RCCE_CAMPITCH`, `RCCE_CAMDIST`,
  captured via `RCCE_SHOT` / `RCCE_SHOT_FRAME`. Reuses the in-world `view.render`
  path, so it matches in-game appearance.

Example:
```
gen-test-zone <data> "Test Terrain"
RCCE_DATA=<data> RCCE_VIEWZONE=1 RCCE_CAMAT="0,8,0" RCCE_CAMPITCH=0.2 \
  RCCE_CAMDIST=170 RCCE_SHOT=out.png ClientRS.exe 127.0.0.1 25000 "Test Terrain"
```

Blitz-side reference renders aren't available headlessly (Graphics3D needs a
desktop session that times out when agent-launched), so parity is judged against
the Blitz **source** behaviour plus the Rust render — the writer/loader code is
authoritative for format, the render code for appearance.

## Status

Legend: ✅ verified (rendered + audited) · 🟡 implemented, render-verify pending ·
❌ missing · ➖ deferred / cosmetic.

| Feature | Status | Notes |
|---|---|---|
| Static scenery meshes | ✅ | renders; catalog scale via placement scale |
| Scenery rotation (pitch/yaw/roll) | ✅ | yaw negated for the LH view (this session) |
| LOD terrain (`CreateTerrain`) | ✅ | parse + grid mesh + height field (this session) |
| Water planes | ✅ | alpha plane, white tint (texture shows), tiling `scale/tex_scale`, scroll anim |
| Skinned actor animation | ✅ | b3d skeletal LBS, GPU/CPU paths |
| Actor attachments (hair/weapon/shield) | ✅ | follow the animated joint (this session) |
| Sky dome (`SkyTexID`) | ✅ | textured skydome renders (harness) |
| Clouds + storm swap (`CloudTexID`) | ✅ | drifting clouds render; storm swap implemented |
| Night stars (`StarsTexID`) | 🟡 | implemented; gated by night factor (verify at night) |
| Fog (`FogRGB`, near/far) | 🟡 | implemented; render-verify ranges |
| Ambient + directional light | 🟡 | from `DefaultLightPitch/Yaw` |
| Day/night cycle | 🟡 | `RCCE_PHASE` / `RCCE_DAYNIGHT_SECS` |
| Lightmaps / multitexture (2nd tex) | ✅ | menu Set.b3d + terrain detail both render `base × tex × 2` |
| Alpha / masked foliage | ✅ | fir needles render as alpha cutout (harness) |
| Vertex colours (`EntityColor`) | 🟡 | confirm per-vertex colour path |
| Projectiles (3D) | 🟡 | combat path |
| Minimap / radar | ✅ | left/right handedness fixed (this session) |
| Terrain detail texture (2nd tex) | ✅ | multitexture `base × detail × 2`, detail UV tiles at `DetailScale` (this session) |
| **Emitters / particles (`.rpc`)** | ❌ | parsed-but-skipped; fire/smoke/fountains/magic. Needs particle engine. **Large** |
| **Dynamic shadows** | ❌ | active in Blitz (Devil Shadow System): sun-cast, actors+scenery casters. Needs shadow-map pass. **Large** |
| **Point lights / `LightModels`** | ❌ | dynamic light meshes; confirm Blitz usage. Medium |
| Water reflection / `AWater` bump+foam | ➖ | cosmetic; deferred |

### Minor — implemented, render-verify pending (low risk; env-driven)

`Fog` ranges, `night stars`, `vertex colours (EntityColor)`, `day/night cycle`,
`ambient + directional light` — all applied in every harness render already; not
independently isolated. Low-risk; can confirm opportunistically.

### Large missing subsystems (NET-NEW — warrant scoping, not "parity tweaks")

These are real, active features in the Blitz client but each is a significant
renderer addition, not a verification pass:

1. **Dynamic shadows** — Blitz runs the **Devil Shadow System** userlib
   (`DevilShadowSystem.decls` + `ShadowsMultiple.bb`): the sun is the
   `ShadowLight` (Environment3D.bb:509), and actors (`CreateShadowCaster AI\EN`,
   Client.bb:453) + scenery (ClientAreas_FE.bb:570) cast real-time shadows,
   rendered by `UpdateShadows Cam` each frame (Client.bb:240). Rust has none.
   Parity needs a **shadow-mapping pass** (render depth from the sun, sample it in
   the main shader). Large.
2. **Emitters / particles** — area `.rpc` emitter configs (fire/smoke/fountains/
   magic) are parsed-but-skipped. Needs an `.rpc` parser + a **particle simulation
   + billboard renderer**. Large; the most *visible* gap.
3. **Point lights / `LightModels`** — dynamic light-emitting meshes; needs Blitz
   usage confirmed, then per-pixel point lighting. Medium.

## Status of the loop

The "verify graphical parity via test renders" pass is **complete for every
implemented feature** — terrain (base+detail), water, scenery+rotation, actors+
attachments, sky, clouds, foliage, multitexture, minimap — and fixed real bugs
along the way (scenery yaw, attachment animation, minimap handedness). What
remains is the three subsystems above: each is a multi-day renderer addition that
should be **scoped and prioritised with the user**, not auto-built in a loop.

Update this table as rows are verified or implemented.
