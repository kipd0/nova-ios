//! Shared listener for the native and compiled-guest ridden-boat tests.
pub fn physics(event: &mut mod_api::ControlledBoatPhysicsEvent<'_>) {
    match event.control.entity {
        30441 if event.control.medium == mod_api::BoatMedium::Land => {
            event.physics.step_height = 1.25;
            event.physics.turn = 3.0;
            event.physics.forward = 0.25;
        }
        30442 if event.control.jump => {
            event.physics.vertical_impulse = 0.5;
            event.physics.gravity = 0.01;
        }
        30443 => {
            event.physics.step_height = f32::INFINITY;
            event.physics.forward = f32::NAN;
            event.physics.drag = -1.0;
            event.physics.gravity = f64::NEG_INFINITY;
        }
        _ => {}
    }
}
