//! Values used by the controlling client's boat tick.
use horizon_mod_api_macros::Wire;

/// The medium sampled under or around the hull before floating and steering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Wire)]
#[non_exhaustive]
pub enum BoatMedium {
    /// Resting on solid collision shapes.
    Land,
    /// Floating on a water surface.
    Water,
    /// Submerged in source water.
    UnderWater,
    /// Submerged in flowing water.
    UnderFlowingWater,
    /// Neither touching land nor in water.
    #[default]
    Air,
}

/// Snapshot after fluid currents, before drag, steering and collision resolution.
#[derive(Clone, Copy, Debug, Default, PartialEq, Wire)]
// exhaustive: constructed by the game across the crate boundary
pub struct BoatControl {
    /// Connection-local entity ID. Clear per-entity settings on removal or disconnect.
    pub entity: u32,
    /// Hull position in blocks.
    pub position: [f64; 3],
    /// Current motion in blocks per tick.
    pub velocity: [f64; 3],
    /// Boat yaw in degrees.
    pub yaw: f32,
    /// Medium sampled for this tick.
    pub medium: BoatMedium,
    /// Whether the previous move ended on the ground.
    pub on_ground: bool,
    /// Latched paddle controls in the order left, right, forward, backward.
    pub paddles: [bool; 4],
    /// Whether the rider holds jump this tick.
    pub jump: bool,
}

/// Coefficients for one locally simulated boat tick. The game supplies the vanilla values.
#[derive(Clone, Copy, Debug, PartialEq, Wire)]
// exhaustive: constructed by the game across the crate boundary
pub struct BoatPhysics {
    /// Downward acceleration in blocks/tick² (signed, -4..=4).
    pub gravity: f64,
    /// Fraction of horizontal motion and turn speed retained (0..=1).
    pub drag: f32,
    /// Water lift fraction, before vanilla's vertical damping (0..=4).
    pub buoyancy: f64,
    /// Turn acceleration in degrees/tick² (0..=180).
    pub turn: f32,
    /// Acceleration when paddling forward in blocks/tick² (-4..=4).
    pub forward: f32,
    /// Acceleration subtracted when paddling backward (-4..=4).
    pub backward: f32,
    /// Forward acceleration from turning without forward/backward input (-4..=4).
    pub paddle_turn: f32,
    /// Extra vertical impulse this tick in blocks/tick (-4..=4). Mods decide when to jump.
    pub vertical_impulse: f64,
    /// Maximum automatic collision step in blocks (0..=4).
    pub step_height: f32,
    /// Include other entities in the hull's movement collision query.
    pub collide_entities: bool,
}

impl Default for BoatPhysics {
    /// Vanilla controls for a boat in air with normal gravity.
    fn default() -> Self {
        Self {
            gravity: 0.04,
            drag: 0.9,
            buoyancy: 0.0,
            turn: 1.0,
            forward: 0.04,
            backward: 0.005,
            paddle_turn: 0.005,
            vertical_impulse: 0.0,
            step_height: 0.0,
            collide_entities: true,
        }
    }
}
