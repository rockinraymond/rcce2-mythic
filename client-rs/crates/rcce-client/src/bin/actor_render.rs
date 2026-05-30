//! Offline actor renderer: load one actor template and render its assembled
//! body + hair + beard to a PNG — no server. Verifies the appearance pipeline
//! (gender mesh, face/body skin, head-attached hair/beard) standalone.
//!
//!   cargo run -p rcce-client --bin actor_render --release -- <templateId> [gender] [out.png]
//!
//! gender: 0 male (default) / 1 female. `RCCE_DATA` overrides the data root.

use rcce_client::assets::{attachment_placement, AssetStore};
use rcce_data::{B3dModel, Image};
use std::rc::Rc;

fn main() {
    let mut args = std::env::args().skip(1);
    let tmpl: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let gender: u8 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let out = args
        .next()
        .unwrap_or_else(|| format!("actor_{tmpl}_g{gender}.png"));

    let data_root = std::env::var("RCCE_DATA")
        .unwrap_or_else(|_| r"C:\Users\dyanr\Desktop\rcce2\data".to_string());
    let mut store = match AssetStore::load(&data_root) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[actor_render] assets: {e}");
            std::process::exit(1);
        }
    };

    let Some(body) = store.actor_model(tmpl, gender) else {
        eprintln!("[actor_render] no body model for template {tmpl} gender {gender}");
        std::process::exit(1);
    };
    let scale = store.actor_render_scale(tmpl, gender).unwrap_or(0.05);
    let body_tex = store.actor_textures(tmpl, gender, 0, 0);
    let ground_y = 0.0f32;
    let (bmin, bmax) = body.bounds();
    let body_trans = [0.0, ground_y - bmin[1] * scale, 0.0];
    let head = body
        .joint_pos("Head")
        .unwrap_or([0.0, bmax[1], 0.0]);
    println!(
        "[actor_render] template {tmpl} gender {gender}: scale {scale:.4}, head joint {head:?}"
    );

    // Pools, mirroring the live client's render assembly.
    let mut models: Vec<Rc<B3dModel>> = vec![body.clone()];
    let mut textures: Vec<Vec<Option<Image>>> = vec![body_tex];
    // (model idx, translation, rot, scale)
    let mut place: Vec<(usize, [f32; 3], [f32; 3], [f32; 3])> = vec![(
        0,
        body_trans,
        [0.0, 0.0, 0.0],
        [scale, scale, scale],
    )];

    for att in store.actor_attachments(tmpl, gender, 0, 0) {
        let (t, r, s) = attachment_placement(body_trans, 0.0, scale, head, &att);
        let idx = models.len();
        println!(
            "[actor_render]   attachment mesh {} ({} sub-meshes) at {t:?} scale {s:?}",
            att.mesh_id,
            att.model.meshes.len()
        );
        models.push(att.model);
        textures.push(att.textures);
        place.push((idx, t, r, s));
    }

    let instances: Vec<rcce_render::SceneInstance> = place
        .iter()
        .map(|&(idx, t, r, s)| rcce_render::SceneInstance {
            model: &models[idx],
            textures: &textures[idx],
            translation: t,
            rot: r,
            scale: s,
            color: [1.0, 1.0, 1.0],
        })
        .collect();

    // Frame the full body height from slightly in front and above.
    let h = ((bmax[1] - bmin[1]) * scale).max(0.5);
    let eye = [h * 1.4, ground_y + h * 0.75, h * 2.4];
    let target = [0.0, ground_y + h * 0.55, 0.0];

    match rcce_render::render_scene_png(&instances, eye, target, ground_y, 900, 1200, &out) {
        Ok(adapter) => println!(
            "[actor_render] rendered {} instances via {adapter} -> {out}",
            instances.len()
        ),
        Err(e) => {
            eprintln!("[actor_render] render failed: {e}");
            std::process::exit(1);
        }
    }
}
