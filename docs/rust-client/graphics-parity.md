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
| Sky dome (`SkyTexID`) | 🟡 | implemented; render-verify in harness |
| Clouds + storm swap (`CloudTexID`) | 🟡 | implemented; render-verify |
| Night stars (`StarsTexID`) | 🟡 | implemented; gated by night factor |
| Fog (`FogRGB`, near/far) | 🟡 | implemented; render-verify ranges |
| Ambient + directional light | 🟡 | from `DefaultLightPitch/Yaw` |
| Day/night cycle | 🟡 | `RCCE_PHASE` / `RCCE_DAYNIGHT_SECS` |
| Lightmaps (2nd-texture multitexture) | 🟡 | menu Set.b3d verified; confirm in-world scenery |
| Alpha / masked foliage | 🟡 | `texture_flag & 4` skip; confirm leaf cutout render |
| Vertex colours (`EntityColor`) | 🟡 | confirm per-vertex colour path |
| Projectiles (3D) | 🟡 | combat path |
| Minimap / radar | ✅ | left/right handedness fixed (this session) |
| **Terrain detail texture (2nd tex)** | ❌ | base-only; Blitz blends detail at `ScaleTexture(DetailScale)` |
| **Emitters / particles (`.rpc`)** | ❌ | parsed-but-skipped; fire/smoke/fountains/magic missing |
| **Actor shadows (`Shadow.bmp` blob)** | ❌ | confirm Blitz draws a ground blob; not in Rust |
| **Point lights / `LightModels`** | ❌ | dynamic light meshes; confirm Blitz usage |
| Water reflection / `AWater` bump+foam | ➖ | cosmetic; deferred |

## Queue (next)

1. Terrain **detail texture** — small; completes the terrain feature.
2. **Alpha foliage** + **lightmap** + **vertex colour** render-verification (one rich test zone).
3. **Actor shadow** blob — check Blitz `Environment3D.bb` / `Shadow.bmp`, add if real.
4. **Emitters / particles** — the largest visible gap; needs `.rpc` config parse + a particle sim.

Update this table as rows are verified or implemented.
