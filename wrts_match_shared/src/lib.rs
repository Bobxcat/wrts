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

pub use pathing::*;

mod pathing {
    use glam::Vec2;
    use splines::{Interpolation, Key, Spline};

    pub struct ShipPathCatmull {
        // spline: Spline,
        knot_spacing: Vec<f32>,
        knots: Vec<Vec2>,
    }

    impl ShipPathCatmull {
        pub fn new(ship_pos: Vec2, ship_dir: Vec2, waypoints: Vec<Vec2>) -> Self {
            let last_knot = *waypoints.last().unwrap();
            let knots = [
                vec![ship_pos, ship_pos + ship_dir],
                waypoints.clone(),
                vec![last_knot],
            ]
            .concat();
            let knot_spacing = [
                vec![0., 0.],
                {
                    let mut x = 0.;
                    let mut v = vec![];
                    let spacing = 1. / waypoints.len() as f32;
                    for _waypoint in waypoints {
                        x += spacing;
                        v.push(x);
                    }
                    v
                },
                vec![1.],
            ]
            .concat();
            assert_eq!(knots.len(), knot_spacing.len());
            Self {
                knot_spacing,
                knots,
            }
        }

        pub fn sample(&self, t: f32) -> Vec2 {
            let spline = Spline::from_vec(
                self.knots
                    .iter()
                    .copied()
                    .zip(self.knot_spacing.iter().copied())
                    .map(|(knot, knot_spacing)| {
                        //
                        Key::new(knot_spacing, knot, Interpolation::CatmullRom)
                    })
                    .collect(),
            );
            spline.clamped_sample(t).unwrap()
        }
    }

    pub struct ShipPathBezier {
        knot_spacing: Vec<f32>,
        knots: Vec<Vec2>,
    }

    impl ShipPathBezier {
        pub fn new(ship_pos: Vec2, ship_dir: Vec2, waypoints: Vec<Vec2>) -> Self {
            let last_knot = *waypoints.last().unwrap();
            let knots = [
                vec![ship_pos, ship_pos + ship_dir],
                waypoints.clone(),
                vec![last_knot],
            ]
            .concat();
            let knot_spacing = [
                vec![0., 0.],
                {
                    let mut x = 0.;
                    let mut v = vec![];
                    let spacing = 1. / waypoints.len() as f32;
                    for _waypoint in waypoints {
                        x += spacing;
                        v.push(x);
                    }
                    v
                },
                vec![1.],
            ]
            .concat();
            assert_eq!(knots.len(), knot_spacing.len());
            Self {
                knot_spacing,
                knots,
            }
        }

        pub fn sample(&self, t: f32) -> Vec2 {
            let spline = Spline::from_vec(
                self.knots
                    .windows(2)
                    .zip(self.knot_spacing.windows(2))
                    .flat_map(|(knot, knot_spacing)| {
                        //
                        let x = (knot[0] + knot[1]) / 2.;
                        [Key::new(knot_spacing[0], knot[0], Interpolation::Bezier(x))]
                    })
                    .collect(),
            );
            spline.clamped_sample(t).unwrap()
        }
    }
}
