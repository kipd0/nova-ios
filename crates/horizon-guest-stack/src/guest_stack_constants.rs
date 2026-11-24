/// At the reporter's measured ceiling of about 1.8 KB per frame, 128 MiB
/// covers about 70k plain frames, including Java's deepest reported 59k case.
#[cfg(not(target_os = "ios"))]
pub const GUEST_STACK_BYTES: usize = 128 * 1024 * 1024;
/// Bound guest depth on iOS so worker stacks can coexist with the integrated server.
#[cfg(target_os = "ios")]
pub const GUEST_STACK_BYTES: usize = 16 * 1024 * 1024;
/// Room for Rust host frames below the guest and synchronous host imports above it.
pub const HOST_HEADROOM: usize = 8 * 1024 * 1024;
