mod germany;
mod japan;
mod russia;
mod sweden;

use std::{f32::consts::PI, time::Duration};

use glam::{EulerRot, Quat, Vec2, Vec3, vec2, vec3};
use paste::paste;
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;

use crate::{formulas::vector_is_within_swept_angle, ship_template::consumables::Consumables};

const SHIP_SPEED_SCALE: f32 = 5.2;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Speed(f32);

impl Speed {
    /// meters per second (game units)
    pub fn from_mps(mps: f32) -> Self {
        Self(mps)
    }

    /// meters per second (game units)
    pub fn mps(self) -> f32 {
        self.0
    }

    /// knots
    pub fn from_kts(kts: f32) -> Self {
        Self(kts / 1.94384)
    }

    /// knots
    pub fn kts(self) -> f32 {
        self.0 * 1.94384
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct AngularSpeed(f32);

impl AngularSpeed {
    /// Radians per second
    pub fn radps(self) -> f32 {
        self.0
    }

    /// Radians per second
    pub fn from_radps(radps: f32) -> Self {
        Self(radps)
    }

    /// Rotations per minute
    pub fn rpm(self) -> f32 {
        self.0 * 30. / PI
    }

    /// Seconds per rotation
    pub fn from_spr(spr: f32) -> Self {
        Self(2. * PI / spr)
    }

    /// Seconds per half turn
    pub fn from_halfturn(sphalfturn: f32) -> Self {
        Self::from_spr(sphalfturn * 0.5)
    }
}

/// Template information
#[derive(Debug)]
pub struct ShipTemplate {
    pub id: ShipTemplateId,
    pub ship_class: ShipClass,
    pub hull: Hull,
    pub max_speed: Speed,
    /// Speed gained per second
    pub engine_acceleration: Speed,
    pub turning_rate: AngularSpeed,
    pub max_health: f64,
    pub detection: f32,
    pub detection_when_firing_through_smoke: f32,
    pub turret_templates: SlotMap<TurretTemplateId, TurretTemplate>,
    pub turret_instances: Vec<TurretInstance>,
    pub torpedoes: Option<Torpedoes>,
    pub consumables: Consumables,
}

/// A unique numerical identifier for each ship template,
/// used for temporary serialization/deserialization.
/// Note that `ShipTemplateId`s can change between versions
///
/// For storage, use [ShipTemplateId::to_name] and [ShipTemplateId::from_name]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShipTemplateId(u32);

macro_rules! make_ship_template_ids {
    (make_ids; $ship_name:ident {$($ship_id_expr:tt)*}) => {
        paste! {
            const [<$ship_name:upper _ID>]: u32 = $($ship_id_expr)*;

            pub const fn $ship_name() -> Self {
                Self(Self::[<$ship_name:upper _ID>])
            }
        }
    };
    (make_ids; $ship_name:ident) => {
        make_ship_template_ids!(make_ids; $ship_name {0});
    };
    (make_ids; $ship_name1:ident $ship_name2:ident  $($others:ident)*) => {
        make_ship_template_ids!(make_ids; $ship_name1 {Self:: [<$ship_name2:upper _ID>]  + 1});
        make_ship_template_ids!(make_ids; $ship_name2 $($others)*);
    };

    (make_name2id; $($ship_names:ident)*) => {
        pub fn from_name(name: &str) -> Option<ShipTemplateId> {
            let name: String = name.to_lowercase();
            Some(match String::as_str(&name) {
                $(stringify!($ship_names) => Self::$ship_names(),)*
                _ => return None,
            })
        }
    };

    (make_id2name; $($ship_names:ident)*) => {
        pub fn to_name(self) -> &'static str {
            paste!{
                match self {
                    $(Self(Self::[<$ship_names:upper _ID>]) => stringify!($ship_names),)*
                    _ => unreachable!("Impossible ShipTemplateId encountered: `{self:?}`"),
                }
            }
        }
    };

    (make_id2template; $($ship_names:ident)*) => {
        pub fn to_template(self) -> &'static ShipTemplate {
            paste!{
                match self {
                    $(Self(Self::[<$ship_names:upper _ID>]) => {
                        static ___STORE: ::std::sync::LazyLock<ShipTemplate> = ::std::sync::LazyLock::new(ShipTemplate::$ship_names);
                        &___STORE
                    },)*
                    _ => unreachable!("Impossible ShipTemplateId encountered: `{self:?}`"),
                }
            }
        }
    };

    (count;) => (0usize);
    (count; $x:tt $($xs:tt)* ) => (1usize + make_ship_template_ids!(count; $($xs)*));

    (make_all_ships; $($ship_names:ident)*) => {
        pub const fn all_ships() -> &'static [ShipTemplateId; const { make_ship_template_ids!(count; $($ship_names)*) }] {
            const {&[$(
                Self::$ship_names()
            ),*]}
        }
    };

    ($($ship_names:ident)*) => {
        impl ShipTemplateId {
            make_ship_template_ids!(make_ids; $($ship_names)*);
            make_ship_template_ids!(make_name2id; $($ship_names)*);
            make_ship_template_ids!(make_id2name; $($ship_names)*);
            make_ship_template_ids!(make_id2template; $($ship_names)*);
            make_ship_template_ids!(make_all_ships; $($ship_names)*);
        }
    };
}

make_ship_template_ids! {
    bismarck

    hipper

    kiev

    nagato
    // north_carolina

    oland
}

impl ShipTemplate {
    pub fn from_name(name: &str) -> Option<&'static Self> {
        ShipTemplateId::from_name(name).map(Self::from_id)
    }

    pub fn from_id(id: ShipTemplateId) -> &'static Self {
        id.to_template()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipClass {
    Battleship,
    CruiserHeavy,
    CruiserLight,
    Destroyer,
}

/// * https://naval-encyclopedia.com/ww2
/// * https://archive.org/details/ship-design-drawings
#[derive(Debug, Clone, Copy)]
pub struct Hull {
    /// Overall length (o/a or "length overall")
    pub length: f32,
    /// The beam of the hull
    pub width: f32,
    /// Height of the hull above the water
    pub freeboard: f32,
    /// Height of the hull below the water
    pub draft: f32,
}

impl Hull {
    /// Returns the bounds of this hull, centered at the origin
    pub fn to_bounds(self) -> (Vec3, Vec3) {
        let min = vec3(-0.5 * self.length, -0.5 * self.width, -self.draft);
        let max = vec3(0.5 * self.length, 0.5 * self.width, self.freeboard);
        (min, max)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HullLocationAxis {
    Centered,
    /// Distance from the back of right of the ship,
    /// offset by the center of this axis
    FromCenter(f32),
    /// Distance from the back or right of the ship
    FromMin(f32),
    /// Distance from the front or left of the ship
    FromMax(f32),
}

impl HullLocationAxis {
    /// Returns the offset on this axis relative to the center of the hull
    fn with_hull_axis(self, hull_length: f32) -> f32 {
        match self {
            HullLocationAxis::Centered => 0.,
            HullLocationAxis::FromCenter(x) => x,
            HullLocationAxis::FromMin(x) => x - 0.5 * hull_length,
            HullLocationAxis::FromMax(x) => 0.5 * hull_length - x,
        }
    }
}

/// The 2d position of an item located on a ship's hull
#[derive(Debug, Clone, Copy)]
pub struct HullLocation {
    /// Along the length of the ship, from back to front
    pub l: HullLocationAxis,
    /// Along the width of the ship, from right to left
    pub w: HullLocationAxis,
}

impl HullLocation {
    pub fn centered() -> Self {
        Self {
            l: HullLocationAxis::Centered,
            w: HullLocationAxis::Centered,
        }
    }

    pub fn new(l: HullLocationAxis, w: HullLocationAxis) -> Self {
        Self { l, w }
    }

    /// `w` will be `Centered`
    pub fn new_l(l: HullLocationAxis) -> Self {
        Self {
            l,
            w: HullLocationAxis::Centered,
        }
    }

    fn to_offset(&self, hull: &Hull) -> Vec2 {
        vec2(
            self.l.with_hull_axis(hull.length),
            self.w.with_hull_axis(hull.width),
        )
    }
    pub fn to_absolute(&self, hull: &Hull, ship_pos: Vec2, ship_rot: Quat) -> Vec2 {
        let (z_rot, _, _) = ship_rot.to_euler(EulerRot::ZXY);
        let rotated = Vec2::from_angle(z_rot).rotate(self.to_offset(hull));
        ship_pos + rotated
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AngleRange {
    from: Vec2,
    to: Vec2,
}

impl AngleRange {
    pub fn start_dir(&self) -> Vec2 {
        self.from
    }

    pub fn end_dir(&self) -> Vec2 {
        self.to
    }

    pub fn from_angles_deg(from: f32, to: f32) -> Self {
        Self::from_angles(from.to_radians(), to.to_radians())
    }

    pub fn from_vectors(from: Vec2, to: Vec2) -> Self {
        Self {
            from: from.normalize(),
            to: to.normalize(),
        }
    }

    /// Returns `self` mirrored across the x axis
    #[must_use]
    pub fn reflect_x(self) -> Self {
        Self::from_vectors(vec2(self.to.x, -self.to.y), vec2(self.from.x, -self.from.y))
    }

    /// An angle range that sweeps counter clockwise
    /// from `from` to `to`
    pub fn from_angles(from: f32, to: f32) -> Self {
        Self {
            from: Vec2::from_angle(from),
            to: Vec2::from_angle(to),
        }
    }

    pub fn rotated_by(self, dir: f32) -> Self {
        let dir = Vec2::from_angle(dir);
        Self {
            from: dir.rotate(self.from),
            to: dir.rotate(self.to),
        }
    }

    pub fn inverse(self) -> Self {
        Self {
            from: self.to,
            to: self.from,
        }
    }

    pub fn contains(self, v: Vec2) -> bool {
        vector_is_within_swept_angle(v, self.from, self.to)
    }

    /// Maintains the length of `v` but clamps its angle
    /// to be within this range of angles
    pub fn clamp_angle(self, v: Vec2) -> Vec2 {
        if self.contains(v) {
            return v;
        }

        if self.from.angle_to(v).abs() > self.to.angle_to(v).abs() {
            self.to * v.length()
        } else {
            self.from * v.length()
        }
    }

    /// Returns whether or not this range of angles overlaps another.
    pub fn overlaps(self, other: Self) -> bool {
        self.contains(other.from) || self.contains(other.to) || other.contains(self.from)
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use glam::{Vec2, vec2};
    use rand::{Rng, rng};

    use crate::ship_template::AngleRange;

    fn random_normalized_vector(rng: &mut impl Rng) -> Vec2 {
        loop {
            if let Some(res) = vec2(rng.random(), rng.random()).try_normalize() {
                return res;
            }
        }
    }

    fn vec2_eq(a: Vec2, b: Vec2) -> bool {
        a.distance_squared(b) <= 0.001
    }

    #[test]
    fn test_angle_to() {
        let v = Vec2::from_angle(0.78);
        assert!(v.angle_to(Vec2::from_angle(3.1187)) > 0.);
        assert!(v.angle_to(Vec2::from_angle(3.1674)) > 0.);
        assert!(v.angle_to(Vec2::from_angle(PI - 0.001).rotate(v)) > 0.);
        assert!(v.angle_to(Vec2::from_angle(PI + 0.001).rotate(v)) < 0.);

        let mut rng = rng();
        for _ in 0..1_000 {
            let v = random_normalized_vector(&mut rng);
            assert!(v.angle_to(Vec2::from_angle(0.001).rotate(v)) > 0.);
            assert!(v.angle_to(Vec2::from_angle(PI - 0.001).rotate(v)) > 0.);
            assert!(v.angle_to(Vec2::from_angle(-0.001).rotate(v)) < 0.);
            assert!(v.angle_to(Vec2::from_angle(PI + 0.001).rotate(v)) < 0.);
        }
    }

    #[test]
    fn test_clamp_angle() {
        let range = AngleRange::from_angles(0.79, 2.3);
        assert!(vec2_eq(
            range.clamp_angle(range.start_dir()),
            range.start_dir()
        ));
        assert!(vec2_eq(range.clamp_angle(range.end_dir()), range.end_dir()));
        assert!(vec2_eq(
            range.clamp_angle(Vec2::from_angle(0.78)),
            range.start_dir()
        ));
        assert!(vec2_eq(
            range.clamp_angle(Vec2::from_angle(2.4)),
            range.end_dir()
        ));
        assert!(vec2_eq(
            range.clamp_angle(Vec2::from_angle(5.45)),
            range.start_dir()
        ));
        assert!(vec2_eq(
            range.clamp_angle(Vec2::from_angle(3.9)),
            range.end_dir()
        ));
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Dispersion {
    /// Vertical radius of the dispersion elliptic cone.
    /// The ellipse is drawn at 1 km
    pub vertical: f32,
    /// Horizontal radius of the dispersion elliptic cone.
    /// The ellipse is drawn at 1 km
    pub horizontal: f32,
    pub sigma: f32,
}

slotmap::new_key_type! {
    pub struct TurretTemplateId;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetingMode {
    /// Only fire at the fire target
    Primary,
    /// Fire at the fire target if possible, otherwise fire
    /// at the closest possible ship
    Secondary,
}

#[derive(Debug, Clone)]
pub struct TurretTemplate {
    pub reload_secs: f32,
    pub damage: f64,
    pub muzzle_vel: f32,
    /// NOTE: a high max_range will not allow a shot to be made past
    /// the 45 degree shell distance at the given muzzle velocity
    pub max_range: f32,
    /// The dispersion per km of shell distance
    pub dispersion: Dispersion,
    pub turn_rate: AngularSpeed,
    /// The list of barrel positions on the turret
    pub barrel_count: u8,
    /// The distance between adjacent barrels on the turret
    pub barrel_spacing: f32,
    pub targeting_mode: TargetingMode,
}

#[derive(Debug, Clone)]
pub struct TurretInstance {
    pub ship_template: ShipTemplateId,
    pub template: TurretTemplateId,
    pub location_on_ship: HullLocation,
    /// If this is `None`, this turret can move in any orientation
    pub movement_angle: Option<AngleRange>,
    /// If this is `None`, the firing angles are equal to the
    /// movement angles
    pub firing_angle: Option<AngleRange>,
    pub default_dir: f32,
}

impl TurretInstance {
    /// Returns this turret reflected across the x axis
    #[must_use]
    fn mirrored(&self) -> Self {
        use HullLocationAxis::*;
        Self {
            ship_template: self.ship_template,
            template: self.template,
            location_on_ship: HullLocation {
                l: self.location_on_ship.l,
                w: match self.location_on_ship.w {
                    Centered => Centered,
                    FromCenter(c) => FromCenter(-c),
                    FromMin(c) => FromMax(c),
                    FromMax(c) => FromMin(c),
                },
            },
            movement_angle: self.movement_angle.map(AngleRange::reflect_x),
            firing_angle: self.firing_angle.map(AngleRange::reflect_x),
            default_dir: Vec2::to_angle(-Vec2::from_angle(self.default_dir)),
        }
    }

    pub fn turret_template(&self) -> &'static TurretTemplate {
        &self.ship_template.to_template().turret_templates[self.template]
    }

    pub fn absolute_pos(&self, ship_pos: Vec2, ship_rot: Quat) -> Vec2 {
        self.location_on_ship.to_absolute(
            &self.ship_template.to_template().hull,
            ship_pos,
            ship_rot,
        )
    }
}

#[derive(Debug)]
pub struct Torpedoes {
    pub reload: Duration,
    pub volleys: usize,
    pub torps_per_volley: usize,
    /// Total radians of torpedo spread
    pub spread: f32,
    pub damage: f64,
    pub speed: Speed,
    pub range: f32,
    pub port_firing_angle: AngleRange,
}

impl Torpedoes {
    pub fn starboard_firing_angle(&self) -> AngleRange {
        self.port_firing_angle.reflect_x()
    }
}

pub mod consumables {
    use std::{num::NonZeroUsize, time::Duration};

    use paste::paste;

    #[derive(Debug, Clone)]
    pub struct Smoke {
        pub action_time: Duration,
        pub dissapation: Duration,
        pub radius: f32,
        pub cooldown: Duration,
        /// Zero if infinite charges
        pub charges: usize,
    }

    #[derive(Debug, Clone)]
    pub struct SpotterPlane {
        pub action_time: Duration,
        pub cooldown: Duration,
        /// Zero if infinite charges
        pub charges: usize,
    }

    macro_rules! make_consumables_struct {
        ($($consumable_type:ident)*) => {
            paste! {
                /// Contains information about the base consumables a ship has access to
                #[derive(Debug, Clone)]
                pub struct Consumables {
                    $([<$consumable_type:snake>] : Option<$consumable_type>),*
                }
            }

            impl Consumables {
                pub fn new() -> Self {
                    paste! {
                        Self {
                            $([<$consumable_type:snake>]: None),*
                        }
                    }
                }

                $(paste! {
                    pub fn [<$consumable_type:snake>](&self) -> Option<&$consumable_type> {
                        self.[<$consumable_type:snake>].as_ref()
                    }


                    pub fn [<with_ $consumable_type:snake>](mut self, [<$consumable_type:snake>]: $consumable_type) -> Self {
                        self.[<$consumable_type:snake>] = Some([<$consumable_type:snake>]);
                        self
                    }
                })*
            }
        };
    }

    make_consumables_struct!(Smoke SpotterPlane);
}
