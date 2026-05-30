//! Minimap / radar projection. Maps a world-space offset from the player into
//! a forward-up radar disc (the direction the camera faces points up), so the
//! render is a pure function of position + camera yaw — testable without a GPU.

/// Project a world delta `(dx, dz)` from the player into radar-disc offset
/// pixels `(ox, oy)` from the centre (+x right, +y down, screen convention),
/// rotated so the camera's forward points up. `range` is the world radius the
/// disc covers; `radius` its pixel radius. Returns `None` when the point falls
/// outside the disc.
///
/// The camera basis matches the movement code (`fwd = (-sin yaw, -cos yaw)`,
/// `right = (cos yaw, -sin yaw)`).
pub fn world_to_radar(dx: f32, dz: f32, yaw: f32, range: f32, radius: f32) -> Option<(f32, f32)> {
    let (s, c) = yaw.sin_cos();
    let along_right = dx * c - dz * s;
    let along_fwd = -dx * s - dz * c;
    let scale = radius / range.max(1.0);
    let ox = along_right * scale;
    let oy = -along_fwd * scale; // forward → up → negative screen-y
    if ox * ox + oy * oy <= radius * radius {
        Some((ox, oy))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: f32 = 72.0;
    const RANGE: f32 = 140.0;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.5
    }

    #[test]
    fn forward_maps_up() {
        // yaw 0 faces -z (fwd = (0,-1)); a target at -z is "in front" → top.
        let (ox, oy) = world_to_radar(0.0, -20.0, 0.0, RANGE, R).unwrap();
        assert!(approx(ox, 0.0), "ox {ox}");
        assert!(oy < 0.0, "front should be up (negative y), got {oy}");
    }

    #[test]
    fn behind_maps_down() {
        let (_, oy) = world_to_radar(0.0, 20.0, 0.0, RANGE, R).unwrap();
        assert!(oy > 0.0, "behind should be down, got {oy}");
    }

    #[test]
    fn yaw_rotates_the_disc() {
        // Same world target, rotate the camera 180°: front becomes back.
        let front = world_to_radar(0.0, -20.0, 0.0, RANGE, R).unwrap();
        let flipped = world_to_radar(0.0, -20.0, std::f32::consts::PI, RANGE, R).unwrap();
        assert!(front.1 < 0.0 && flipped.1 > 0.0, "{front:?} vs {flipped:?}");
    }

    #[test]
    fn out_of_range_is_none() {
        assert!(world_to_radar(1000.0, 0.0, 0.0, RANGE, R).is_none());
    }

    #[test]
    fn scales_with_distance() {
        let near = world_to_radar(0.0, -20.0, 0.0, RANGE, R).unwrap();
        let far = world_to_radar(0.0, -40.0, 0.0, RANGE, R).unwrap();
        assert!(far.1 < near.1, "farther in front sits higher (more negative y)");
    }
}
