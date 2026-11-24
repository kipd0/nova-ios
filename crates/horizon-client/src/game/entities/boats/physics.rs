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
        ..BoatPhysics::default()
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

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::geometry::Vec3;

    #[test]
    fn vanilla_coefficients_match_the_boat_tick_in_each_medium() {
        let mut boat = Boat::new(1, "oak".into(), Vec3::ZERO, 0.0);
        boat.water_level = HEIGHT / 2.0;
        boat.land_friction = 0.6;
        for (medium, gravity, drag, buoyancy) in [
            (Status::InAir, 0.04, 0.9, 0.0),
            (Status::OnLand, 0.04, 0.6, 0.0),
            (Status::InWater, 0.04, 0.9, 0.5),
            (Status::UnderFlowingWater, 7.0e-4, 0.9, 0.0),
            (Status::UnderWater, 0.04, 0.45, f64::from(0.01_f32)),
        ] {
            boat.status = medium;
            let p = vanilla(&boat);
            assert_eq!((p.gravity, p.drag, p.buoyancy), (gravity, drag, buoyancy));
            assert_eq!(
                (p.turn, p.forward, p.backward, p.paddle_turn),
                (1.0, 0.04, 0.005, 0.005)
            );
            assert_eq!(
                (p.step_height, p.vertical_impulse, p.collide_entities),
                (0.0, 0.0, true)
            );
        }
    }

    #[test]
    fn invalid_outputs_restore_only_the_affected_fields() {
        let base = BoatPhysics {
            drag: 0.6,
            ..Default::default()
        };
        let mut physics = BoatPhysics {
            gravity: f64::NEG_INFINITY,
            drag: -1.0,
            buoyancy: f64::NAN,
            turn: 181.0,
            forward: f32::NAN,
            backward: -4.1,
            paddle_turn: 4.1,
            vertical_impulse: f64::INFINITY,
            step_height: 1.25,
            collide_entities: false,
        };
        validate(&mut physics, base);
        assert_eq!(
            physics,
            BoatPhysics {
                step_height: 1.25,
                collide_entities: false,
                ..base
            }
        );
        physics.step_height = f32::INFINITY;
        validate(&mut physics, base);
        assert_eq!(physics.step_height, 0.0);
    }
}
