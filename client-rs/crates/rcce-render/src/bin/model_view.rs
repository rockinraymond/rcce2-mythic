//! Render a real `.b3d` model to a PNG. Verifies the B3D parser + 3D renderer
//! together. `cargo run --release -p rcce-render --bin model-view -- <file.b3d> [out.png]`

use rcce_data::B3dModel;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| {
        r"C:\Users\dyanr\Desktop\rcce2\data\Meshes\Actors\Animals\stag.b3d".to_string()
    });
    let out = args.next().unwrap_or_else(|| "model.png".to_string());

    let bytes = std::fs::read(&path).unwrap_or_else(|e| {
        eprintln!("[model-view] cannot read {path}: {e}");
        std::process::exit(2);
    });
    let model = B3dModel::parse(&bytes).unwrap_or_else(|e| {
        eprintln!("[model-view] parse failed: {e}");
        std::process::exit(1);
    });
    println!(
        "[model-view] {path}: {} meshes, {} verts, {} tris, {} textures, {} brushes",
        model.meshes.len(),
        model.vertex_count(),
        model.triangle_count(),
        model.textures.len(),
        model.brushes.len(),
    );
    for (i, m) in model.meshes.iter().enumerate() {
        println!("[model-view]   mesh {i}: {} verts, brush {}, texture {:?}", m.positions.len(), m.brush_id, m.texture);
    }
    match rcce_render::render_model_png(&model, 0.6, 900, 900, &out) {
        Ok(adapter) => println!("[model-view] rendered via {adapter} -> {out}"),
        Err(e) => {
            eprintln!("[model-view] render failed: {e}");
            std::process::exit(1);
        }
    }
}
