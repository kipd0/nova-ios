//! Boundary between the client boat simulation and the mod API.
use super::{Boat, HEIGHT, Status};
use horizon_client::entity_rules::EntityRules;
use mod_api::{BoatControl, BoatMedium, BoatPhysics};

pub(super) fn vanilla(b: &Boat) -> BoatPhysics {
    let mut gravity = b.get_gravity();
    let mut buoyancy = 0.0;
    let drag = match b.status {
        Status::InWater => {
            buoyancy = (b.water_level - b.position.y) / HEIGHT;
            0.9
        }
        Status::UnderFlowingWater => {
            gravity = 7.0e-4;
            0.9
        }
        Status::UnderWater => {
            buoyancy = f64::from(0.01_f32);
            0.45
        }
        Status::InAir => 0.9,
        Status::OnLand => b.land_friction,
    };
    BoatPhysics {
        gravity,
        drag,
        buoyancy,
        turn: 1.0,
        forward: 0.04,
        backward: 0.005,
        paddle_turn: 0.005,
        vertical_impulse: 0.0,
        step_height: 0.0,
        collide_entities: true,
    }
}

pub(super) fn control(b: &Boat, jump: bool) -> BoatControl {
    BoatControl {
        entity: b.id,
        position: [b.position.x, b.position.y, b.position.z],
        velocity: [b.velocity.x, b.velocity.y, b.velocity.z],
        yaw: b.yaw,
        medium: match b.status {
            Status::OnLand => BoatMedium::Land,
            Status::InWater => BoatMedium::Water,
            Status::UnderWater => BoatMedium::UnderWater,
            Status::UnderFlowingWater => BoatMedium::UnderFlowingWater,
            Status::InAir => BoatMedium::Air,
        },
        on_ground: b.on_ground,
        paddles: b.input,
        jump,
    }
}

pub(super) fn validate(p: &mut BoatPhysics, base: BoatPhysics) {
    fn bounded<T: PartialOrd>(value: &mut T, range: std::ops::RangeInclusive<T>, original: T) {
        if !range.contains(value) {
            *value = original;
        }
    }
    bounded(&mut p.gravity, -4.0..=4.0, base.gravity);
    bounded(&mut p.buoyancy, 0.0..=4.0, base.buoyancy);
    bounded(&mut p.drag, 0.0..=1.0, base.drag);
    bounded(&mut p.turn, 0.0..=180.0, base.turn);
    bounded(&mut p.forward, -4.0..=4.0, base.forward);
    bounded(&mut p.backward, -4.0..=4.0, base.backward);
    bounded(&mut p.paddle_turn, -4.0..=4.0, base.paddle_turn);
    bounded(&mut p.vertical_impulse, -4.0..=4.0, base.vertical_impulse);
    bounded(&mut p.step_height, 0.0..=4.0, base.step_height);
}
