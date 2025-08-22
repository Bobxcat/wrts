use glam::Vec2;

pub mod ship_template;

/// (lower_bound, higher_bound)
///
/// It's a 48km square centered on the origin
///
/// Note that in WoWS, all the largest maps are listed as 48km but
/// are actually 24km due to the game scaling
pub fn map_bounds() -> (Vec2, Vec2) {
    let size = 48_000.;
    let half = size / 2.;
    (Vec2::splat(-half), Vec2::splat(half))
}
