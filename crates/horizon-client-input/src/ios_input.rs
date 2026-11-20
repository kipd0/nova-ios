//! Nova iOS input bridge. The gameplay runtime calls these safe functions;
//! only the iOS host's C ABI needs raw pointers. Nothing runs on desktop.
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug)]
pub enum Input {
    Key(u32, bool),
    Mouse(u32, bool),
    Cursor(f64, f64),
    Motion(f64, f64),
    Text(String),
    KeyboardClosed,
    Close,
}

static INPUTS: OnceLock<Mutex<Vec<Input>>> = OnceLock::new();
static TOUCH_GESTURES: AtomicBool = AtomicBool::new(true);
static OVERLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
static IN_WORLD: AtomicBool = AtomicBool::new(false);
static POINTER_STATE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Low bit indicates first-person gameplay, high bits identify a transition
/// out of first-person. Navigation between menus must preserve cursor position.
pub fn publish_pointer_mode(in_world: bool, changed_screen: bool) {
    let previous = POINTER_STATE.load(Ordering::Relaxed);
    let epoch = (previous >> 1).wrapping_add(u64::from(changed_screen));
    POINTER_STATE.store((epoch << 1) | u64::from(in_world), Ordering::Release);
}

pub fn pointer_state() -> u64 {
    POINTER_STATE.load(Ordering::Acquire)
}

pub fn set_playing(value: bool) {
    IN_WORLD.store(value, Ordering::Relaxed);
}

pub fn playing() -> bool {
    IN_WORLD.load(Ordering::Relaxed)
}

pub fn set_overlay_active(value: bool) {
    OVERLAY_ACTIVE.store(value, Ordering::Release);
}

pub fn overlay_active() -> bool {
    OVERLAY_ACTIVE.load(Ordering::Acquire)
}

fn input_queue() -> &'static Mutex<Vec<Input>> {
    INPUTS.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn push(event: Input) {
    if let Ok(mut inputs) = input_queue().lock() {
        // Ignore excessive events instead of exhausting the main thread.
        if inputs.len() < 2048 {
            match event {
                Input::Motion(dx, dy) => {
                    if let Some(Input::Motion(x, y)) = inputs.last_mut() {
                        *x += dx;
                        *y += dy;
                    } else {
                        inputs.push(Input::Motion(dx, dy));
                    }
                }
                Input::Cursor(x, y) => {
                    if let Some(Input::Cursor(old_x, old_y)) = inputs.last_mut() {
                        *old_x = x;
                        *old_y = y;
                    } else {
                        inputs.push(Input::Cursor(x, y));
                    }
                }
                other => inputs.push(other),
            }
        }
    }
}

pub fn take() -> Vec<Input> {
    input_queue()
        .lock()
        .map(|mut inputs| std::mem::take(&mut *inputs))
        .unwrap_or_default()
}

pub fn set_touch_gestures(value: bool) {
    TOUCH_GESTURES.store(value, Ordering::Relaxed);
}

pub fn touch_gestures() -> bool {
    TOUCH_GESTURES.load(Ordering::Relaxed)
}
