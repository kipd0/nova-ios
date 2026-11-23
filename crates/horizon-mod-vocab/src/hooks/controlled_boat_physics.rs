//! The controlling client's boat simulation.
pub trait Hooks {
    /// Adjust the coefficients and collision policy for one locally driven boat or raft,
    /// after sampling its medium and before movement. The resolved hull drives the rider,
    /// rendering and the ordinary vehicle movement packet. The server retains movement authority.
    /// Each invalid field falls back to its vanilla value. Water entry keeps the vanilla surface
    /// snap in place of the drag/gravity/buoyancy update for that tick.
    #[hook(side = "client", contract = "listen")]
    fn controlled_boat_physics();
}
