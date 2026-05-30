//! Data-driven asset resolution: actor template id → base mesh id (`Actors.dat`)
//! → `.b3d` path (`Meshes.dat` catalog) → parsed [`B3dModel`], with caching.
//! Reads the same files the GUE editor writes, so the client draws each actor
//! as its real model.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use rcce_data::{
    texture, ActorCatalog, AnimClip, AnimSetCatalog, B3dModel, Image, MeshCatalog, TextureCatalog,
};

pub struct AssetStore {
    data_root: PathBuf,
    actors: ActorCatalog,
    meshes: MeshCatalog,
    textures: TextureCatalog,
    anims: AnimSetCatalog,
    cache: HashMap<u16, Option<Rc<B3dModel>>>,
}

/// The looping frame for a clip at `elapsed` seconds, given the timeline `fps`.
/// Honors the clip's speed; single-frame clips return their start.
pub fn clip_frame(clip: &AnimClip, fps: f32, elapsed: f32) -> f32 {
    let len = (clip.end - clip.start).max(0) as f32;
    if len <= 0.0 {
        return clip.start as f32;
    }
    let advanced = elapsed * fps.max(0.001) * clip.speed.max(0.001);
    clip.start as f32 + advanced.rem_euclid(len + 1.0)
}

/// A head attachment (hair or beard): its model + textures, plus the mesh
/// catalog's own position offset and scale (the engine's `LoadedMeshX/Y/Z` and
/// `LoadedMeshScales`, applied relative to the body's `Head` joint).
pub struct Attachment {
    pub mesh_id: u16,
    pub model: Rc<B3dModel>,
    pub textures: Vec<Option<Image>>,
    pub offset: [f32; 3],
    pub scale: f32,
}

/// World placement (translation, rot radians, per-axis scale) for an attachment
/// parented to the body's `Head` joint. Mirrors the engine: the attachment sits
/// at `head_offset + catalog_offset` in the actor's scaled, yaw-rotated frame.
/// `body_translation` is the body instance's ground-seated world translation;
/// `yaw` matches `glam::Mat4::from_rotation_y`.
pub fn attachment_placement(
    body_translation: [f32; 3],
    yaw: f32,
    actor_scale: f32,
    head_offset: [f32; 3],
    att: &Attachment,
) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let lx = (head_offset[0] + att.offset[0]) * actor_scale;
    let ly = (head_offset[1] + att.offset[1]) * actor_scale;
    let lz = (head_offset[2] + att.offset[2]) * actor_scale;
    let (s, c) = yaw.sin_cos(); // from_rotation_y: x' = x c + z s, z' = -x s + z c
    let translation = [
        body_translation[0] + lx * c + lz * s,
        body_translation[1] + ly,
        body_translation[2] - lx * s + lz * c,
    ];
    let scale = actor_scale * att.scale;
    (translation, [0.0, yaw, 0.0], [scale, scale, scale])
}

impl AssetStore {
    /// `data_root` is the project `data/` directory (containing `Server Data/`,
    /// `Game Data/`, `Meshes/`).
    pub fn load(data_root: impl Into<PathBuf>) -> Result<Self, String> {
        let data_root = data_root.into();
        let actors_bytes = std::fs::read(data_root.join("Server Data/Actors.dat"))
            .map_err(|e| format!("Actors.dat: {e}"))?;
        let mesh_bytes = std::fs::read(data_root.join("Game Data/Meshes.dat"))
            .map_err(|e| format!("Meshes.dat: {e}"))?;
        let actors = ActorCatalog::parse(&actors_bytes).map_err(|e| format!("Actors.dat: {e}"))?;
        let meshes = MeshCatalog::parse(&mesh_bytes)
            .map_err(|e| format!("Meshes.dat: {e}"))?
            .value;
        // Texture catalog (for real actor skins). Non-fatal if absent.
        let textures = std::fs::read(data_root.join("Game Data/Textures.dat"))
            .ok()
            .and_then(|b| TextureCatalog::parse(&b).ok())
            .map(|p| p.value)
            .unwrap_or_default();
        // Animation-set table (named clip ranges). Non-fatal if absent.
        let anims = std::fs::read(data_root.join("Game Data/Animations.dat"))
            .ok()
            .and_then(|b| AnimSetCatalog::parse(&b).ok())
            .unwrap_or_default();
        Ok(Self {
            data_root,
            actors,
            meshes,
            textures,
            anims,
            cache: HashMap::new(),
        })
    }

    /// The animation clip for an actor's named state ("Idle", "Walk", "Run",
    /// "Default attack", "Death 1", …), resolved through the actor's animation
    /// set (per gender). Tries the names in order; `None` if unmatched.
    pub fn actor_clip(&self, template_id: u16, gender: u8, names: &[&str]) -> Option<&AnimClip> {
        let t = self.actors.templates.get(&template_id)?;
        let set_id = if gender == 1 { t.f_anim_set } else { t.m_anim_set };
        let set = self.anims.get(set_id)?;
        // Exact (case-insensitive) match wins over a fuzzy substring — so
        // "Idle" picks "Idle", not "Sit idle"; "Run" picks "Run", not "Ride run".
        for n in names {
            if let Some(c) = set.clip(n).filter(|c| c.end >= c.start) {
                return Some(c);
            }
        }
        set.find(names).filter(|c| c.end >= c.start)
    }

    /// Template gender-mode (`Actors.dat` `Genders`) for every actor template,
    /// keyed by id. The packet decoder needs this to know whether a P_NewActor
    /// carries a gender byte (only when mode == 0).
    pub fn template_genders(&self) -> HashMap<u16, u8> {
        self.actors
            .templates
            .iter()
            .map(|(&id, t)| (id, t.genders))
            .collect()
    }

    /// Resolve a texture-catalog id to an on-disk path under `data/Textures/`.
    fn skin_path(&self, id: u16) -> Option<PathBuf> {
        if id == 65535 {
            return None;
        }
        let entry = self.textures.get(id)?;
        let p = self
            .data_root
            .join("Textures")
            .join(entry.filename.replace('\\', "/"));
        p.exists().then_some(p)
    }

    /// The base body model for an actor template + gender (0 male / 1 female).
    pub fn actor_model(&mut self, template_id: u16, gender: u8) -> Option<Rc<B3dModel>> {
        let mesh_id = self.actors.mesh_for(template_id, gender)?;
        self.mesh_model(mesh_id)
    }

    /// The actor's in-world render scale, matching the engine
    /// (`Actors3D.bb:45`): `0.05 × LoadedMeshScales[mesh] × Actor.Scale`.
    /// Positions stay in raw world units. Falls back to `0.05` if a stored
    /// scale is non-positive.
    pub fn actor_render_scale(&self, template_id: u16, gender: u8) -> Option<f32> {
        let mesh_id = self.actors.mesh_for(template_id, gender)?;
        let mesh = self.meshes.get(mesh_id)?;
        let actor = self.actors.templates.get(&template_id)?;
        let ms = if mesh.scale > 0.0 { mesh.scale } else { 1.0 };
        let as_ = if actor.scale > 0.0 { actor.scale } else { 1.0 };
        Some(0.05 * ms * as_)
    }

    /// Textures for an actor's model, one per mesh (aligned to
    /// `actor_model(...).meshes`). Each mesh's B3D texture filename is resolved
    /// by basename against the mesh's own directory and the project texture
    /// trees, then decoded (BMP/PNG/JPG). `None` where unresolved/undecodable.
    pub fn actor_textures(
        &mut self,
        template_id: u16,
        gender: u8,
        face_sel: u8,
        body_sel: u8,
    ) -> Vec<Option<Image>> {
        let Some(mesh_id) = self.actors.mesh_for(template_id, gender) else {
            return Vec::new();
        };

        // Real skins from this actor's chosen body/face texture selection
        // (0..4). These replace the b3d's embedded UV-guide textures.
        let fi = (face_sel as usize).min(4);
        let bi = (body_sel as usize).min(4);
        let (face_skin, body_skin) = match self.actors.templates.get(&template_id) {
            Some(t) => {
                let (faces, bodies) = if gender == 1 {
                    (t.female_face_ids, t.female_body_ids)
                } else {
                    (t.male_face_ids, t.male_body_ids)
                };
                (self.skin_path(faces[fi]), self.skin_path(bodies[bi]))
            }
            None => (None, None),
        };

        // Fallback search roots for the b3d's own textures.
        let mut roots = Vec::new();
        if let Some(entry) = self.meshes.get(mesh_id) {
            let rel = entry.filename.replace('\\', "/");
            if let Some(dir) = self.data_root.join("Meshes").join(&rel).parent() {
                roots.push(dir.to_path_buf());
            }
        }
        roots.push(self.data_root.join("Textures"));
        roots.push(self.data_root.join("Meshes"));

        let Some(model) = self.mesh_model(mesh_id) else {
            return Vec::new();
        };
        model
            .meshes
            .iter()
            .map(|m| {
                // Surface type from the b3d texture name: head/face vs body.
                let is_face = m
                    .texture
                    .as_deref()
                    .map(texture::basename)
                    .map(|b| {
                        let l = b.to_ascii_lowercase();
                        l.contains("head") || l.contains("face")
                    })
                    .unwrap_or(false);
                let skin = if is_face { &face_skin } else { &body_skin };
                // Prefer the real actor skin; fall back to the b3d's texture.
                skin.as_ref()
                    .and_then(|p| texture::load(p))
                    .or_else(|| {
                        m.texture
                            .as_ref()
                            .and_then(|name| texture::find_texture(&roots, name))
                            .and_then(|p| texture::load(&p))
                    })
            })
            .collect()
    }

    /// Textures for a plain scenery mesh, one per sub-mesh (aligned to
    /// `mesh_model(mesh_id).meshes`). Resolves each sub-mesh's own B3D texture
    /// name against the mesh directory and the project texture trees. If
    /// `retexture_id` is a real texture-catalog id (not 65535), it overrides
    /// every sub-mesh (the engine's scenery `TextureID` retexture).
    pub fn scenery_textures(&mut self, mesh_id: u16, retexture_id: u16) -> Vec<Option<Image>> {
        // Optional whole-mesh retexture from the area file's TextureID.
        let retex = self
            .skin_path(retexture_id)
            .and_then(|p| texture::load(&p));

        // Search roots: the mesh's own directory, then the texture trees.
        let mut roots = Vec::new();
        if let Some(entry) = self.meshes.get(mesh_id) {
            let rel = entry.filename.replace('\\', "/");
            if let Some(dir) = self.data_root.join("Meshes").join(&rel).parent() {
                roots.push(dir.to_path_buf());
            }
        }
        roots.push(self.data_root.join("Textures"));
        roots.push(self.data_root.join("Meshes"));

        let Some(model) = self.mesh_model(mesh_id) else {
            return Vec::new();
        };
        model
            .meshes
            .iter()
            .map(|m| {
                if let Some(img) = &retex {
                    return Some(img.clone());
                }
                m.texture
                    .as_ref()
                    .and_then(|name| texture::find_texture(&roots, name))
                    .and_then(|p| texture::load(&p))
            })
            .collect()
    }

    /// Head attachments (hair, and beard for males) for an actor, resolved from
    /// the template's Hair/Beard selection (0..4). Empty when the slots are
    /// unset (65535) or the meshes don't resolve.
    pub fn actor_attachments(
        &mut self,
        template_id: u16,
        gender: u8,
        hair_sel: u8,
        beard_sel: u8,
    ) -> Vec<Attachment> {
        let Some(t) = self.actors.templates.get(&template_id).cloned() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let hair_ids = if gender == 1 {
            t.female_hair_ids
        } else {
            t.male_hair_ids
        };
        if let Some(a) = self.mesh_attachment(hair_ids[(hair_sel as usize).min(4)]) {
            out.push(a);
        }
        // Beards are male-only.
        if gender != 1 {
            if let Some(a) = self.mesh_attachment(t.beard_ids[(beard_sel as usize).min(4)]) {
                out.push(a);
            }
        }
        out
    }

    /// Build an [`Attachment`] for a mesh-catalog id (its model + textures +
    /// catalog offset/scale). `None` for the 65535 "none" slot or a miss.
    fn mesh_attachment(&mut self, mesh_id: u16) -> Option<Attachment> {
        if mesh_id == 65535 {
            return None;
        }
        let entry = self.meshes.get(mesh_id)?.clone();
        let model = self.mesh_model(mesh_id)?;
        let textures = self.scenery_textures(mesh_id, 65535);
        Some(Attachment {
            mesh_id,
            model,
            textures,
            offset: entry.offset,
            scale: if entry.scale > 0.0 { entry.scale } else { 1.0 },
        })
    }

    /// A model by mesh-catalog id, cached (including negative cache for misses).
    pub fn mesh_model(&mut self, mesh_id: u16) -> Option<Rc<B3dModel>> {
        if let Some(cached) = self.cache.get(&mesh_id) {
            return cached.clone();
        }
        let result = self
            .meshes
            .get(mesh_id)
            .and_then(|entry| {
                let path = self
                    .data_root
                    .join("Meshes")
                    .join(entry.filename.replace('\\', "/"));
                std::fs::read(path).ok()
            })
            .and_then(|bytes| B3dModel::parse(&bytes).ok())
            .map(Rc::new);
        self.cache.insert(mesh_id, result.clone());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcce_data::AnimClip;

    #[test]
    fn clip_frame_loops_within_range() {
        let clip = AnimClip { name: "Walk".into(), start: 10, end: 20, speed: 1.0 };
        // t=0 -> start.
        assert!((clip_frame(&clip, 10.0, 0.0) - 10.0).abs() < 1e-4);
        // Always within [start, end] (inclusive-ish, len+1 wrap).
        for i in 0..200 {
            let f = clip_frame(&clip, 10.0, i as f32 * 0.05);
            assert!(f >= 10.0 && f < 21.0, "frame {f} out of [10,21)");
        }
        // Single-frame clip pins to start.
        let one = AnimClip { name: "Sit".into(), start: 142, end: 142, speed: 1.0 };
        assert_eq!(clip_frame(&one, 30.0, 5.0), 142.0);
    }

    fn att(offset: [f32; 3], scale: f32) -> Attachment {
        Attachment {
            mesh_id: 1,
            model: Rc::new(B3dModel::default()),
            textures: Vec::new(),
            offset,
            scale,
        }
    }

    #[test]
    fn attachment_placement_no_yaw() {
        // Head 100 up, no catalog offset, actor scale 0.05 -> head sits at
        // body_y + 5; attachment scale = actor_scale * catalog_scale.
        let (t, r, s) = attachment_placement([10.0, 0.0, 20.0], 0.0, 0.05, [0.0, 100.0, 0.0], &att([0.0, 0.0, 0.0], 2.0));
        assert!((t[0] - 10.0).abs() < 1e-4);
        assert!((t[1] - 5.0).abs() < 1e-4);
        assert!((t[2] - 20.0).abs() < 1e-4);
        assert_eq!(r, [0.0, 0.0, 0.0]);
        assert!((s[0] - 0.1).abs() < 1e-4);
    }

    #[test]
    fn attachment_placement_yaw_rotates_offset() {
        // A +Z head offset under a 90° yaw rotates to +X (glam from_rotation_y).
        use std::f32::consts::FRAC_PI_2;
        let (t, _r, _s) = attachment_placement([0.0, 0.0, 0.0], FRAC_PI_2, 1.0, [0.0, 0.0, 10.0], &att([0.0, 0.0, 0.0], 1.0));
        assert!((t[0] - 10.0).abs() < 1e-3, "x={}", t[0]);
        assert!(t[1].abs() < 1e-3);
        assert!(t[2].abs() < 1e-3, "z={}", t[2]);
    }
}
