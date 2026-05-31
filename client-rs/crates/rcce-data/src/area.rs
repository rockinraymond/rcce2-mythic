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
    /// Cloud / storm-cloud / night-stars texture ids (Textures.dat; 65535 = none).
    /// Drawn as slowly-drifting sky overlays (`Environment3D.bb` CloudEN/StarsEN).
    pub cloud_tex_id: u16,
    pub storm_cloud_tex_id: u16,
    pub stars_tex_id: u16,
    /// `LoadingMusicID` — indexes `Music.dat` for the zone's looping track
    /// (65535 = none).
    pub music_id: u16,
    /// Fog colour (0..1). Also the natural sky/clear colour.
    pub fog_color: [f32; 3],
    pub fog_near: f32,
    pub fog_far: f32,
    pub ambient: [f32; 3],
    /// Unit vector *toward* the zone's directional light (for diffuse shading),
    /// derived from the stored `DefaultLightPitch`/`Yaw` the engine feeds to
    /// `RotateEntity(DefaultLight, pitch, yaw, 0)`.
    pub light_dir: [f32; 3],
    pub outdoors: bool,
}

/// Toward-light unit vector from the engine's pitch/yaw (degrees). The Blitz
/// directional light shines along its rotated local +Z; shading wants the
/// opposite (the direction light arrives from).
pub fn light_dir_from_pitch_yaw(pitch_deg: f32, yaw_deg: f32) -> [f32; 3] {
    let (p, y) = (pitch_deg.to_radians(), yaw_deg.to_radians());
    // Forward (shine) = (cosP·sinY, -sinP, cosP·cosY); toward-light = -forward.
    [-(p.cos() * y.sin()), p.sin(), -(p.cos() * y.cos())]
}

impl Default for AreaEnv {
    fn default() -> Self {
        AreaEnv {
            sky_tex_id: 65535,
            cloud_tex_id: 65535,
            storm_cloud_tex_id: 65535,
            stars_tex_id: 65535,
            music_id: 65535,
            fog_color: [0.45, 0.62, 0.82],
            fog_near: 1000.0,
            fog_far: 8000.0,
            ambient: [0.5, 0.5, 0.5],
            light_dir: [0.0, 0.5, -0.866],
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
            r.seek(2)?; // skip LoadingTexID (i16@0)
            let music_id = r.read_short_u()?; // LoadingMusicID (i16@2)
            let sky_tex_id = r.read_short_u()?; // @4
            let cloud_tex_id = r.read_short_u()?; // @6
            let storm_cloud_tex_id = r.read_short_u()?; // @8
            let stars_tex_id = r.read_short_u()?; // @10 (now at @12 = FogRGB)
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
            // DefaultLightPitch, DefaultLightYaw (degrees), SlopeRestrict follow.
            let light_pitch = r.read_float()?;
            let light_yaw = r.read_float()?;
            let light_dir = light_dir_from_pitch_yaw(light_pitch, light_yaw);
            Ok(AreaEnv {
                sky_tex_id,
                cloud_tex_id,
                storm_cloud_tex_id,
                stars_tex_id,
                music_id,
                fog_color,
                fog_near,
                fog_far,
                ambient,
                light_dir,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: [f32; 3], b: [f32; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4)
    }

    #[test]
    fn light_dir_default_pitch() {
        // The engine default (pitch 30, yaw 0): light from above-and-behind.
        let l = light_dir_from_pitch_yaw(30.0, 0.0);
        assert!(approx(l, [0.0, 0.5, -0.8660254]), "got {l:?}");
        // Always a unit vector.
        let mag = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        assert!((mag - 1.0).abs() < 1e-4, "mag {mag}");
    }

    #[test]
    fn light_dir_straight_down() {
        // Pitch 90: light straight overhead → toward-light points +Y.
        let l = light_dir_from_pitch_yaw(90.0, 0.0);
        assert!(approx(l, [0.0, 1.0, 0.0]), "got {l:?}");
    }

    #[test]
    fn light_dir_yaw_rotates_horizontal() {
        // Pitch 0, yaw 90: purely horizontal, rotated onto -X.
        let l = light_dir_from_pitch_yaw(0.0, 90.0);
        assert!(approx(l, [-1.0, 0.0, 0.0]), "got {l:?}");
    }
}
