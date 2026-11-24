use super::*;

impl Game<'_> {
    /// Read the locally driven boat and edit this tick's coefficients.
    pub(in crate::game) fn native_controlled_boat_physics(
        &self,
        control: &mod_api::BoatControl,
        physics: &mut mod_api::BoatPhysics,
    ) {
        self.fire_controlled_boat_physics(control, physics);
    }
}
