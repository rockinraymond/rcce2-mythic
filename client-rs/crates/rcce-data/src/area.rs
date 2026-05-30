//! Client area (`Data/Areas/<zone>.dat`) — the VISUAL zone data the client
//! loads (`ClientAreas_FE.bb::LoadArea`). We parse the scenery placement list
//! (the props/terrain meshes that fill the world); the rest of the header is
//! display/environment settings the renderer doesn't need yet.
//!
//! The area file's header (the Width..ShadowR fields in LoadArea come from
//! Options.dat, NOT this file) is a fixed 41-byte prefix:
//! LoadingTexID,LoadingMusicID,SkyTexID,CloudTexID,StormCloudTexID,StarsTexID
//! (i16×6) · FogR,G,B(u8×3) · FogNear,FogFar(f32×2) · MapTexID(i16) ·
//! Outdoors(u8) · AmbientR,G,B(u8×3) · DefaultLightPitch,Yaw,SlopeRestrict(f32×3).
//! Then `Sceneries:i16`, then each record:
//! MeshID(i16) · X,Y,Z(f32) · Pitch,Yaw,Roll(f32) · ScaleX,Y,Z(f32) ·
//! AnimMode(u8) · SceneryID(u8) · TextureID(i16) · CatchRain(u8) · Collides(u8) ·
//! Lightmap(str) · RCTE(str) · CastShadow(u8) · ReceiveShadow(u8) · RenderRange(u8).

use crate::reader::{BlitzReader, ReadError};

/// One placed scenery object (a mesh-catalog id at a world transform).
#[derive(Debug, Clone)]
pub struct SceneryPlacement {
    pub mesh_id: u16,
    pub pos: [f32; 3],
    /// Pitch, Yaw, Roll in degrees.
    pub rot: [f32; 3],
    pub scale: [f32; 3],
    /// Optional retexture id (texture catalog), 65535/none if unused.
    pub texture_id: u16,
}

/// Zone environment/atmosphere from the area header — what the renderer needs
/// for sky colour, distance fog, and ambient light.
#[derive(Debug, Clone)]
pub struct AreaEnv {
    pub sky_tex_id: u16,
    /// Fog colour (0..1). Also the natural sky/clear colour.
    pub fog_color: [f32; 3],
    pub fog_near: f32,
    pub fog_far: f32,
    pub ambient: [f32; 3],
    pub outdoors: bool,
}

impl Default for AreaEnv {
    fn default() -> Self {
        AreaEnv {
            sky_tex_id: 65535,
            fog_color: [0.45, 0.62, 0.82],
            fog_near: 1000.0,
            fog_far: 8000.0,
            ambient: [0.5, 0.5, 0.5],
            outdoors: true,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct AreaScenery {
    pub env: AreaEnv,
    pub sceneries: Vec<SceneryPlacement>,
}

/// Byte offset of the `Sceneries` count (fixed header prefix length).
const SCENERY_COUNT_OFFSET: usize = 41;

impl AreaScenery {
    pub fn parse(data: &[u8]) -> Result<AreaScenery, ReadError> {
        let mut r = BlitzReader::new(data);
        // Header (41 bytes): 6×i16 tex/music ids · FogRGB(u8×3) · FogNear,Far
        // (f32×2) · MapTexID(i16) · Outdoors(u8) · AmbientRGB(u8×3) · light(f32×3).
        let env = (|| -> Result<AreaEnv, ReadError> {
            r.seek(4)?; // skip LoadingTexID, LoadingMusicID
            let sky_tex_id = r.read_short_u()?;
            r.seek(12)?; // to FogRGB
            let fog_color = [
                r.read_byte()? as f32 / 255.0,
                r.read_byte()? as f32 / 255.0,
                r.read_byte()? as f32 / 255.0,
            ];
            let fog_near = r.read_float()?;
            let fog_far = r.read_float()?;
            r.seek(25)?; // skip MapTexID(i16) to Outdoors
            let outdoors = r.read_byte()? != 0;
            let ambient = [
                r.read_byte()? as f32 / 255.0,
                r.read_byte()? as f32 / 255.0,
                r.read_byte()? as f32 / 255.0,
            ];
            Ok(AreaEnv {
                sky_tex_id,
                fog_color,
                fog_near,
                fog_far,
                ambient,
                outdoors,
            })
        })()
        .unwrap_or_default();

        r.seek(SCENERY_COUNT_OFFSET)?;
        let count = r.read_short_u()?;
        let mut sceneries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let mesh_id = r.read_short_u()?;
            let pos = [r.read_float()?, r.read_float()?, r.read_float()?];
            let rot = [r.read_float()?, r.read_float()?, r.read_float()?];
            let scale = [r.read_float()?, r.read_float()?, r.read_float()?];
            let _anim_mode = r.read_byte()?;
            let _scenery_id = r.read_byte()?;
            let texture_id = r.read_short_u()?;
            let _catch_rain = r.read_byte()?;
            let _collides = r.read_byte()?;
            let _lightmap = r.read_string(260)?;
            let _rcte = r.read_string(260)?;
            let _cast_shadow = r.read_byte()?;
            let _receive_shadow = r.read_byte()?;
            let _render_range = r.read_byte()?;
            sceneries.push(SceneryPlacement {
                mesh_id,
                pos,
                rot,
                scale,
                texture_id,
            });
        }
        Ok(AreaScenery { env, sceneries })
    }
}
