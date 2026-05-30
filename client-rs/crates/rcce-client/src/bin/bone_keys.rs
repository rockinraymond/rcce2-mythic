//! Inspect a b3d's bones and their keyframes in a frame window, to diagnose
//! skinning: which bones are weighted, how dense their keys are, and the key
//! frame numbers around a target frame.
//!
//!   cargo run -p rcce-client --bin bone_keys --release -- <b3d> <loFrame> <hiFrame> [nameSubstr]

use rcce_data::B3dModel;

fn main() {
    let mut a = std::env::args().skip(1);
    let path = a.next().unwrap_or_else(|| {
        r"C:\Users\dyanr\Desktop\rcce2\data\Meshes\Actors\Humans\Male_02.b3d".to_string()
    });
    let lo: i32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let hi: i32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(25);
    let filt = a.next().unwrap_or_default().to_ascii_lowercase();

    let model = B3dModel::parse(&std::fs::read(&path).expect("read")).expect("parse");
    println!("{path}: {} bones", model.bones.len());

    for b in &model.bones {
        if !filt.is_empty() && !b.name.to_ascii_lowercase().contains(&filt) {
            continue;
        }
        let total = b.keys.len();
        let in_win: Vec<i32> = b
            .keys
            .iter()
            .map(|k| k.frame)
            .filter(|&f| f >= lo && f <= hi)
            .collect();
        let span = if total > 0 {
            format!("{}..{}", b.keys.first().unwrap().frame, b.keys.last().unwrap().frame)
        } else {
            "—".into()
        };
        println!(
            "bone '{}' parent={:?} weights={} keys={} span[{}] in[{lo}..{hi}]={:?}",
            b.name, b.parent, b.weights.len(), total, span, in_win
        );
        // Compare the NODE bind local quaternion to the first key's rotation —
        // if frame 1 is the rest pose they should match (reveals order/sign).
        if !filt.is_empty() {
            println!("    bind local_r (w,x,y,z) = {:?}", b.local_r);
            println!("    bind local_t          = {:?}", b.local_t);
            for k in b.keys.iter().filter(|k| k.frame >= lo && k.frame <= hi) {
                println!(
                    "    f{:<4} rot={:?} pos={:?} scale={:?}",
                    k.frame,
                    k.rotation.map(|r| r.map(|v| (v * 1000.0).round() / 1000.0)),
                    k.position.map(|p| p.map(|v| (v * 100.0).round() / 100.0)),
                    k.scale.map(|s| s.map(|v| (v * 100.0).round() / 100.0)),
                );
            }
        }
    }
}
