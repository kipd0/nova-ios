//! The loading screen while the resources load (`Loading`), the window's app before the game
//! takes over (`App`), and the checks a start makes first: missing or damaged prepared files,
//! an unclean last exit, a fullscreen the window system could not show.
use super::*;

/// The window system ended the event loop with a failure (on Linux, a compositor dropping the
/// connection after a protocol error) while fullscreen was on: the next start opens windowed, so a
/// fullscreen the system can't show doesn't crash every launch (#2904).
pub(super) fn leave_fullscreen_after(
    result: &std::result::Result<(), EventLoopError>,
    settings: &mut Settings,
) {
    if matches!(result, Err(EventLoopError::ExitFailure(_))) && settings.fullscreen {
        settings.fullscreen = false;
        settings.save(&Game::options_path());
        log::warn!(
            "the window system closed the game in fullscreen; the next start opens windowed"
        );
    }
}

pub(super) fn nova_diag(stage: &str) {
    use std::io::Write as _;
    if let Some(path) = std::env::var_os("NOVA_DIAGNOSTIC_LOG") {
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(path) {
            let _ = writeln!(f, "[engine] {stage}");
        }
    }
}

pub(super) fn run_event_loop(mut game: Game<'_>) -> Result<()> {
    if game.options.headless {
        // No display to open winit's loop on: the game's own loop.
        game.run_headless();
        // narrator::start ran for it too (options.txt's narrator setting).
        narrator::stop();
        let result = write_report(&game).and_then(|()| game.error.take().map_or(Ok(()), Err));
        if game.options.is_client_start() {
            game.native_client_stopped();
            game.native_game_shutting_down();
        }
        return result;
    }
    let mut split = split::Split::new(game);
    let result = EventLoop::new()?.run_app(&mut split);
    game = split.host;
    narrator::stop();
    leave_fullscreen_after(&result, &mut game.settings);
    let result = result
        .map_err(Into::into)
        .and_then(|()| write_report(&game))
        .and_then(|()| game.error.take().map_or(Ok(()), Err));
    if game.options.is_client_start() {
        game.native_client_stopped();
        game.native_game_shutting_down();
    }
    result
}

/// The loading screen's state while the worker loads the resources.
pub(super) struct Loading {
    pub(super) window: Option<Arc<Window>>,
    pub(super) renderer: Option<Renderer>,
    /// The boot screen's atlas, until the pack's GUI atlas replaces it.
    pub(super) boot: horizon_client::hud::HudAtlas,
    pub(super) gui: Option<UiResources>,
    pub(super) settings: Settings,
    pub(super) fullscreen: horizon_client::window::FullscreenState,
    pub(super) video_modes: horizon_client::window::VideoModeList,
    pub(super) options: Option<Options>,
    pub(super) vanilla_root: PathBuf,
    pub(super) pack_error: Option<String>,
    pub(super) started: Instant,
    pub(super) panorama_rotation: horizon_client::gfx::panorama::Rotation,
    pub(super) progress: Progress,
    pub(super) receiver: Receiver<LoaderMsg>,
    pub(super) loaded: Option<std::result::Result<Loaded, String>>,
    /// The loader's `Done` arrived: its channel closing afterwards is expected.
    pub(super) delivered: bool,
    pub(super) handle: Option<JoinHandle<()>>,
    pub(super) shown: bool,
    /// When the menu bundle arrived (`set_ui`), and whether a title frame with
    /// it was presented since: the world renderer waits for that frame
    /// (`ready_for_handoff`).
    pub(super) ui_at: Option<Instant>,
    pub(super) title_drawn: bool,
    pub(super) menu: startup_menu::StartupMenu,
    pub(super) error: Option<Box<dyn std::error::Error>>,
}
/// How long the handoff waits for the first title frame before building the
/// world renderer anyway (a hidden window may never be asked to redraw).
pub(super) const TITLE_FRAME_WAIT: Duration = Duration::from_secs(1);
impl Loading {
    /// The world renderer is built on the main thread, so building it before
    /// the title is on the screen holds the title back by that build. Wait for
    /// one title frame first, unless a world is waiting to open.
    pub(super) fn ready_for_handoff(&self) -> bool {
        self.title_drawn
            || self.menu.pending.is_some()
            || self.ui_at.is_none_or(|at| at.elapsed() >= TITLE_FRAME_WAIT)
    }
    pub(super) fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        nova_diag("winit.resumed");
        if self.window.is_some() {
            return;
        }
        // The Window constructor: created fullscreen when options.txt or --fullscreen says so.
        let fullscreen =
            self.settings.fullscreen || self.options.as_ref().is_some_and(|o| o.fullscreen);
        let size = self
            .options
            .as_ref()
            .map_or([1280, 720], |o| o.window_size)
            .map(f64::from);
        self.fullscreen = horizon_client::window::FullscreenState::new(fullscreen);
        let attributes =
            horizon_client::window::with_icon(Window::default_attributes().with_title("Horizon"));
        #[cfg(target_os = "ios")]
        let attributes = {
            use winit::platform::ios::{
                ScreenEdge, ValidOrientations, WindowAttributesExtIOS,
            };
            attributes
                .with_prefers_status_bar_hidden(true)
                .with_prefers_home_indicator_hidden(true)
                .with_preferred_screen_edges_deferring_system_gestures(ScreenEdge::ALL)
                .with_valid_orientations(ValidOrientations::Landscape)
        };
        // Wayland names the window by its app id, which the horizon.desktop entry matches.
        #[cfg(target_os = "linux")]
        let attributes = winit::platform::wayland::WindowAttributesExtWayland::with_name(
            attributes, "horizon", "horizon",
        );
        // `--offscreen` renders into a texture: its window would only show white.
        let window_step = startup::step("window.create");
        nova_diag("window.create.begin");
        // UIKit owns the iOS window bounds: desktop --window-size creates a
        // 16:9 Metal surface on a 4:3 iPad and can crop the title/buttons.
        #[cfg(target_os = "ios")]
        let attributes = attributes
            .with_visible(!self.options.as_ref().is_some_and(|o| o.offscreen));
        #[cfg(not(target_os = "ios"))]
        let attributes = attributes
            .with_inner_size(winit::dpi::LogicalSize::new(size[0], size[1]))
            .with_fullscreen(fullscreen.then_some(winit::window::Fullscreen::Borderless(None)))
            .with_visible(!self.options.as_ref().is_some_and(|o| o.offscreen));
        let window = match event_loop.create_window(attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                nova_diag(&format!("window.create.failed: {e}"));
                self.error = Some(e.into());
                event_loop.exit();
                return;
            }
        };
        nova_diag("window.create.succeeded");
        #[cfg(not(target_os = "ios"))]
        horizon_client::window::settle_created_fullscreen(&window, fullscreen);
        #[cfg(target_os = "ios")]
        nova_diag(&format!(
            "ios.window.bounds; physical={}x{}; scale={}",
            window.inner_size().width,
            window.inner_size().height,
            window.scale_factor()
        ));
        #[cfg(target_os = "windows")]
        if let Some(hwnd) = hwnd(&window) {
            horizon_threads::system_menu::suppress(hwnd);
        }
        drop(window_step);
        // The renderer takes seconds to build and winit's window sits white
        // until the first frame presents, so paint the boot screen on it and
        // keep repainting it as the console's lines arrive; the wgpu frames
        // redraw the same design when they start. GDI is unsafe:
        // horizon-threads holds it (this crate forbids unsafe code).
        #[cfg(target_os = "windows")]
        // In the fonts given to `menu_style`; with none, the window stays as it is.
        let boot_paint_stop = hwnd(&window)
            .zip(horizon_client::menu_style::get().fonts)
            .map(|(hwnd, fonts)| {
                let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let repaint_stop = stop.clone();
                std::thread::spawn(move || {
                    while !repaint_stop.load(std::sync::atomic::Ordering::Relaxed) {
                        horizon_threads::boot_paint::paint(
                            hwnd,
                            fonts.semibold,
                            fonts.mono,
                            &horizon_client::logging::recent_lines(),
                        );
                        std::thread::sleep(std::time::Duration::from_millis(300));
                    }
                });
                stop
            });
        let offscreen = self.options.as_ref().is_some_and(|o| o.offscreen);
        let boot_step = startup::step("renderer.boot");
        nova_diag("renderer.boot.begin");
        match loading_renderer(
            window.clone(),
            offscreen,
            Some(&self.boot),
            None,
            None,
            self.settings.horizon.effective_api(),
            shader_packs::picked_pack_shape(&self.settings).ray_tracing,
        ) {
            Ok(r) => {
                nova_diag("renderer.boot.succeeded");
                self.renderer = Some(r)
            },
            Err(e) => {
                #[cfg(target_os = "windows")]
                if let Some(stop) = &boot_paint_stop {
                    stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                nova_diag(&format!("renderer.boot.failed: {e}"));
                self.error = Some(e);
                event_loop.exit();
                return;
            }
        }
        drop(boot_step);
        // The wgpu frames take the window over: stop the boot paint thread
        // before the first one presents.
        #[cfg(target_os = "windows")]
        if let Some(stop) = &boot_paint_stop {
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }

        self.window = Some(window);
    }
    pub(super) fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        mut event: WindowEvent,
    ) {
        if self.window.as_ref().is_none_or(|w| w.id() != id) {
            return;
        }
        follow_keymap(&mut event);
        if let Err(error) = self.menu_event(event_loop, event) {
            self.error = Some(error);
            event_loop.exit();
        }
    }
    /// The menu bundle arrived: rebuild the loading screen with the real atlas
    /// and title panorama.
    pub(super) fn set_ui(&mut self, gui: UiResources, root: PathBuf) {
        nova_diag("resources.menu-received");
        log::info!(
            "Horizon: loading the title artwork after {} ms",
            self.started.elapsed().as_millis()
        );
        let additional = self
            .options
            .as_ref()
            .map_or_else(Vec::new, |options| pack_finders::data_packs(&options.mods));
        self.menu
            .set_root(root, &self.settings, additional.into(), &gui.splashes);
        self.gui = Some(gui);
        self.title_drawn = false;
        horizon_client::ui_theme::reset_atlas();
        self.apply_menu_settings();
        let Some(window) = self.window.clone() else {
            return;
        };
        let offscreen = self.options.as_ref().is_some_and(|o| o.offscreen);
        if let Some(renderer) = &mut self.renderer {
            renderer.release_surface();
        }
        let bundle = self.gui.as_ref().expect("the menu bundle was just set");
        let _step = startup::step("renderer.title_artwork");
        match loading_renderer(
            window,
            offscreen,
            Some(&bundle.atlas),
            Some(&bundle.panorama),
            self.renderer.as_ref(),
            self.settings.horizon.effective_api(),
            shader_packs::picked_pack_shape(&self.settings).ray_tracing,
        ) {
            Ok(r) => {
                self.renderer = Some(r);
                // The GPU holds the atlas now: keep its copy compressed until the world
                // renderer reads it at the handoff, as the game does after its first frame.
                if let Some(gui) = &mut self.gui {
                    gui.atlas.pack_pixels();
                }
            }
            Err(e) => {
                nova_diag(&format!("renderer.menu.failed: {e}"));
                self.error = Some(e)
            },
        }
        // The title frame's wait starts once its renderer is built.
        self.ui_at = Some(Instant::now());
    }
    pub(super) fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // The loop may run once more after exit(): keep the first error, the
        // one that ended loading.
        if self.error.is_some() {
            event_loop.exit();
            return;
        }
        if horizon_threads::stop_requests() > 0 {
            // No world is open yet: nothing to save.
            event_loop.exit();
            return;
        }
        if let Err(error) = self.poll_menu_resources() {
            self.error = Some(error);
            event_loop.exit();
            return;
        }
        self.poll_menu_realms(event_loop);
        if let Some(w) = &self.window {
            if self.options.as_ref().is_some_and(|o| o.offscreen) {
                self.draw();
            } else {
                w.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::Poll);
    }
    pub(super) fn draw(&mut self) {
        let Some(size) = self
            .renderer
            .as_ref()
            .map(|r| [r.config.width, r.config.height])
        else {
            return;
        };
        let mut batch = HudBatch::new(size, self.settings.gui_scale_inputs());
        self.draw_menu(&mut batch);
        self.menu.frame_drawn((batch.width, batch.height));
        let mut frame = loading_frame(&batch);
        frame.yaw = self.panorama_rotation.update(
            self.started.elapsed().as_secs_f32(),
            self.settings.file.double("panoramaScrollSpeed") as f32,
        );
        if let Some(renderer) = &mut self.renderer {
            match renderer.render(&frame) {
                Ok(true) if self.gui.is_some() && self.menu.pending.is_none() => {
                    #[cfg(target_os = "ios")]
                    NOVA_IOS_LOADING_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if !self.title_drawn { nova_diag("renderer.title.frame-presented"); }
                    self.title_drawn = true;
                }
                Ok(true) => {
                    #[cfg(target_os = "ios")]
                    NOVA_IOS_LOADING_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                Ok(_) => {}
                Err(e) => log::warn!("Horizon: loading screen frame failed: {e}"),
            }
        }
        if !self.shown {
            nova_diag("renderer.first-frame-attempted");
            self.shown = true;
            startup::mark("loading_screen.shown");
            log::info!(
                "Horizon: loading screen shown after {} ms",
                self.started.elapsed().as_millis()
            );
        }
    }
    pub(super) fn finish(&mut self, mut loaded: Loaded) -> Result<Option<split::Split<'static>>> {
        // iOS: the initial world-rest worker must publish its tables before
        // Game setup reads them. The loading loop waited for completion below,
        // so joining here should not block the UIKit event loop.
        #[cfg(target_os = "ios")]
        if let Some(loader) = loaded.rest_loader.take() {
            #[cfg(target_os = "ios")]
            NOVA_IOS_PHASE.store(1, std::sync::atomic::Ordering::Relaxed);
            eprintln!("[nova-watchdog] handoff.join.begin");
            loader.finish()?;
            eprintln!("[nova-watchdog] handoff.join.done");
            eprintln!("Nova iOS: world-rest tables joined before game handoff");
        }
        let handoff_at = Instant::now();
        let mut options = self.options.take().ok_or("loading already finished")?;
        options.resources = loaded.root.clone();
        // StartupMenu owns world selection until it can hand it to Game.
        options.world = None;
        options.server = None;
        let gui = self.gui.take().ok_or("missing menu bundle")?;
        let window = self.window.clone().ok_or("window closed")?;
        // Keep the loader's device and swap in the real textures in one step. The loading
        // screen draws no frame after this: its atlas texture is freed before the world
        // renderer uploads its own, so a phone never holds both (#15608).
        let mut previous = self.renderer.take();
        if let Some(renderer) = &mut previous {
            renderer.release_surface();
            if let Some(atlas) = &renderer.hud.atlas_texture {
                atlas.destroy();
            }
        }
        let renderer_at = Instant::now();
        let renderer = startup::time("renderer.world", || {
            build_renderer(
                window.clone(),
                options.offscreen,
                &loaded.pack,
                &gui,
                previous.as_ref(),
                loaded.renderer_cpu,
                self.settings.horizon.effective_api(),
                self.settings.horizon.translucent_batching,
                self.settings.vsync,
                &self.settings.horizon,
                shader_packs::picked_pack_shape(&self.settings),
                previous
                    .as_ref()
                    .map(|r| r.display_reading())
                    .unwrap_or_default(),
                (
                    self.settings.file.int("textureFiltering"),
                    self.settings.file.int("maxAnisotropyBit"),
                ),
            )
        })?;
        log::info!(
            "Horizon: world renderer ready after {} ms",
            self.started.elapsed().as_millis()
        );
        let renderer_ms = renderer_at.elapsed().as_millis();
        // Audio is created on the main thread, after the core is ready.
        let audio = if !options.execution_role.is_client() {
            Audio::silent(&loaded.pack.source)
        } else {
            Audio::with_device(&loaded.pack.source, &self.settings.sound_device)
        };
        // The menu read its language from the prepared root alone (`set_root`) and
        // `load_resources` skips it on this path: read it again with the mods' layers, in the
        // language picked in the menu.
        horizon_client::lang::set(horizon_client::lang::Language::load(
            &[&loaded.pack.source],
            &horizon_client::lang::code(),
        ));
        let pack_error = loaded.pack_error.or_else(|| self.pack_error.take());
        // The game borrows its shared resources for the process's lifetime,
        // like the single copy an integrated server would share.
        let res: &'static Arc<Resources> = Box::leak(Box::new(loaded.resources));
        let game_at = Instant::now();
        let game_step = startup::step("handoff.start_game");
        let Some(mut game) = start_game(
            res,
            loaded.pack,
            gui,
            options,
            pack_error,
            audio,
            self.vanilla_root.clone(),
            self.started,
            loaded.rest_loader,
            Some(self.settings.clone()),
            loaded.game_cpu,
            std::mem::take(&mut loaded.client_applies),
            loaded.entity_models,
        )?
        else {
            return Ok(None);
        };
        drop(game_step);
        let game_ms = game_at.elapsed().as_millis();
        game.renderer = Some(renderer);
        game.resize_state = Some(game.resize_state_of(window.inner_size()));
        game.window = Some(window);
        game.world_renderer_ready = true;
        game.fullscreen = self.fullscreen;
        game.panorama_rotation = self.panorama_rotation;
        self.menu.handoff(&mut game);
        game.apply_settings();
        game.previous = Instant::now();
        // Minecraft.onGameLoadFinished: a start that made it this far is clean.
        game.settings.file.set(
            "startedCleanly",
            horizon_client::options_file::V::Bool(true),
        );
        game.settings.save(&Game::options_path());
        log::info!(
            "Horizon: startup handoff {} ms (renderer {} ms, start_game {} ms)",
            handoff_at.elapsed().as_millis(),
            renderer_ms,
            game_ms
        );
        Ok(Some(split::Split::new(game)))
    }
}

/// The application: the loading screen until the resources are ready, then
/// the game itself, on the same event loop and window.
pub(super) enum App {
    Loading(Box<Loading>),
    Ready(Box<split::Split<'static>>),
}
// Nova iOS-only adaptation of Kelly-Inc/horizon-ios's stacker::maybe_grow.
// The iOS main thread is ~1 MiB and renderer initialization overflows it.
// Unlike std::thread::spawn, this keeps UIKit and Metal on the same OS thread.
#[inline(never)]
fn nova_ios_deep<T>(work: impl FnOnce() -> T) -> T {
    #[cfg(target_os = "ios")]
    {
        stacker::maybe_grow(1 << 20, 32 << 20, work)
    }
    #[cfg(not(target_os = "ios"))]
    {
        work()
    }
}


// Nova iOS diagnostic counters. The independent monitor never calls UIKit,
// winit, or Metal, and does not wake or change the game's event loop.
#[cfg(target_os = "ios")]
static NOVA_IOS_WATCHDOG_ONCE: std::sync::Once = std::sync::Once::new();
#[cfg(target_os = "ios")]
static NOVA_IOS_PHASE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(target_os = "ios")]
static NOVA_IOS_IDLE_BEGIN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(target_os = "ios")]
static NOVA_IOS_IDLE_END: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(target_os = "ios")]
static NOVA_IOS_REDRAW_BEGIN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(target_os = "ios")]
static NOVA_IOS_REDRAW_END: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(target_os = "ios")]
static NOVA_IOS_LOADING_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(target_os = "ios")]
static NOVA_IOS_GAME_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(target_os = "ios")]
fn nova_ios_start_watchdog(progress: std::sync::Arc<std::sync::atomic::AtomicU32>) {
    use std::sync::atomic::Ordering::Relaxed;
    NOVA_IOS_WATCHDOG_ONCE.call_once(|| {
        let task = move || {
            let mut previous = [0u64; 6];
            loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let current = [
                    NOVA_IOS_IDLE_BEGIN.load(Relaxed),
                    NOVA_IOS_IDLE_END.load(Relaxed),
                    NOVA_IOS_REDRAW_BEGIN.load(Relaxed),
                    NOVA_IOS_REDRAW_END.load(Relaxed),
                    NOVA_IOS_LOADING_FRAMES.load(Relaxed),
                    NOVA_IOS_GAME_FRAMES.load(Relaxed),
                ];
                eprintln!(
                    "[nova-watchdog] phase={} progress={}/9 idle={} (+{}) returned={} (+{}) redraw={} (+{}) completed={} (+{}) loading_frames={} (+{}) game_frames={} (+{})",
                    NOVA_IOS_PHASE.load(Relaxed),
                    progress.load(Relaxed),
                    current[0], current[0].saturating_sub(previous[0]),
                    current[1], current[1].saturating_sub(previous[1]),
                    current[2], current[2].saturating_sub(previous[2]),
                    current[3], current[3].saturating_sub(previous[3]),
                    current[4], current[4].saturating_sub(previous[4]),
                    current[5], current[5].saturating_sub(previous[5]),
                );
                previous = current;
            }
        };
        if let Err(error) = std::thread::Builder::new()
            .name("nova-ios-watchdog".into())
            .spawn(task)
        {
            eprintln!("[nova-watchdog] could not start: {error}");
        }
    });
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "ios")]
        nova_diag("ios.deep-stack.resumed.begin");
        #[cfg(target_os = "ios")]
        if let App::Loading(loading) = self {
            nova_ios_start_watchdog(loading.progress.value.clone());
        }
        nova_ios_deep(|| self.resumed_here(event_loop));
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        #[cfg(target_os = "ios")]
        let redraw = matches!(&event, WindowEvent::RedrawRequested);
        #[cfg(target_os = "ios")]
        if redraw { NOVA_IOS_REDRAW_BEGIN.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
        nova_ios_deep(|| self.window_event_here(event_loop, id, event));
        #[cfg(target_os = "ios")]
        if redraw { NOVA_IOS_REDRAW_END.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
    }
    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        nova_ios_deep(|| self.device_event_here(event_loop, id, event));
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "ios")]
        NOVA_IOS_IDLE_BEGIN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        nova_ios_deep(|| self.about_to_wait_here(event_loop));
        #[cfg(target_os = "ios")]
        {
            if let App::Ready(game) = self {
                NOVA_IOS_GAME_FRAMES.store(game.host.frames as u64, std::sync::atomic::Ordering::Relaxed);
            }
            NOVA_IOS_IDLE_END.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        nova_ios_deep(|| self.exiting_here(event_loop));
    }
}

impl App {
    #[cfg(target_os = "ios")]
    fn ios_key_code(code: u32) -> Option<KeyCode> {
        Some(match code {
            65 => KeyCode::KeyA,
            66 => KeyCode::KeyB,
            67 => KeyCode::KeyC,
            68 => KeyCode::KeyD,
            69 => KeyCode::KeyE,
            70 => KeyCode::KeyF,
            71 => KeyCode::KeyG,
            72 => KeyCode::KeyH,
            73 => KeyCode::KeyI,
            74 => KeyCode::KeyJ,
            75 => KeyCode::KeyK,
            76 => KeyCode::KeyL,
            77 => KeyCode::KeyM,
            78 => KeyCode::KeyN,
            79 => KeyCode::KeyO,
            80 => KeyCode::KeyP,
            81 => KeyCode::KeyQ,
            82 => KeyCode::KeyR,
            83 => KeyCode::KeyS,
            84 => KeyCode::KeyT,
            85 => KeyCode::KeyU,
            86 => KeyCode::KeyV,
            87 => KeyCode::KeyW,
            88 => KeyCode::KeyX,
            89 => KeyCode::KeyY,
            90 => KeyCode::KeyZ,
            48 => KeyCode::Digit0,
            49 => KeyCode::Digit1,
            50 => KeyCode::Digit2,
            51 => KeyCode::Digit3,
            52 => KeyCode::Digit4,
            53 => KeyCode::Digit5,
            54 => KeyCode::Digit6,
            55 => KeyCode::Digit7,
            56 => KeyCode::Digit8,
            57 => KeyCode::Digit9,
            1001 => KeyCode::Space,
            1002 => KeyCode::ShiftLeft,
            1003 => KeyCode::ControlLeft,
            1004 => KeyCode::Escape,
            1005 => KeyCode::Enter,
            1006 => KeyCode::Tab,
            1007 => KeyCode::Backspace,
            1008 => KeyCode::ArrowUp,
            1009 => KeyCode::ArrowDown,
            1010 => KeyCode::ArrowLeft,
            1011 => KeyCode::ArrowRight,
            1012 => KeyCode::F1,
            1013 => KeyCode::F2,
            1014 => KeyCode::F3,
            1015 => KeyCode::F5,
            1016 => KeyCode::F11,
            1017 => KeyCode::Slash,
            1018 => KeyCode::F4,
            1019 => KeyCode::F6,
            1020 => KeyCode::F7,
            1021 => KeyCode::F8,
            1022 => KeyCode::F9,
            1023 => KeyCode::F10,
            1024 => KeyCode::F12,
            1025 => KeyCode::AltLeft,
            1026 => KeyCode::ShiftRight,
            1027 => KeyCode::ControlRight,
            1028 => KeyCode::AltRight,
            1029 => KeyCode::Comma,
            1030 => KeyCode::Period,
            1031 => KeyCode::Minus,
            1032 => KeyCode::Equal,
            1033 => KeyCode::Backquote,
            1034 => KeyCode::BracketLeft,
            1035 => KeyCode::BracketRight,
            1036 => KeyCode::Backslash,
            1037 => KeyCode::Semicolon,
            1038 => KeyCode::Quote,
            1039 => KeyCode::CapsLock,
            _ => return None,
        })
    }

    #[cfg(target_os = "ios")]
    fn ios_drain_inputs(&mut self, event_loop: &ActiveEventLoop) {
        use horizon_client_input::ios_input::{self, Input};
        use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
        static WAS_MODAL: AtomicBool = AtomicBool::new(false);
        let modal = ios_input::overlay_active();
        // Only recenter on the transition from grabbed gameplay back
        // into a GUI, e.g. opening inventory or pause. Options opened
        // from the main menu preserves the existing cursor position.
        let in_world = matches!(self, App::Ready(game)
            if game.host.screen == Screen::Playing
                && game.host.title.is_none()
                && !game.host.screen_over_playing());
        static WAS_IN_WORLD: AtomicBool = AtomicBool::new(false);
        let was_in_world = WAS_IN_WORLD.swap(in_world, AtomicOrdering::Relaxed);
        let changed = was_in_world && !in_world;
        ios_input::set_playing(in_world);
        ios_input::publish_pointer_mode(in_world, changed);
        let was_modal = WAS_MODAL.swap(modal, AtomicOrdering::Relaxed);
        if modal && !was_modal {
            if let App::Ready(game) = self {
                // A mod can reject a gameplay press and still observe its raw key event.
                let held: Vec<KeyCode> = game.host.physical_keys
                    .union(&game.host.rejected_keys)
                    .copied()
                    .collect();
                for key in held {
                    // Release keys through the same iOS path so Shift/Ctrl
                    // modifier bits cannot remain stuck while Nova is open.
                    game.host.ios_virtual_key(event_loop, key, false);
                }
            }
            let id = match self {
                App::Loading(loading) => loading.window.as_ref().map(|w| w.id()),
                App::Ready(game) => game.host.window.as_ref().map(|w| w.id()),
            };
            if let Some(id) = id {
                for button in [MouseButton::Left, MouseButton::Right] {
                    self.window_event_here(event_loop, id, WindowEvent::MouseInput {
                        device_id: DeviceId::dummy(),
                        state: ElementState::Released,
                        button,
                    });
                }
            }
        }
        if modal {
            // Discard all queued controls, chat and look movement during Nova UI.
            let _ = ios_input::take();
            return;
        }
        let id = match self {
            App::Loading(loading) => loading.window.as_ref().map(|w| w.id()),
            App::Ready(game) => game.host.window.as_ref().map(|w| w.id()),
        };
        let Some(id) = id else { return };
        for input in horizon_client_input::ios_input::take() {
            match input {
                Input::Key(code, down) => {
                    let Some(key) = Self::ios_key_code(code) else { continue };
                    // Winit KeyEvent has private platform fields: use Horizon's
                    // supported mapped-key input path instead of fabricating one.
                    match self {
                        App::Ready(game) => {
                            game.host.ios_virtual_key(event_loop, key, down);
                        }
                        App::Loading(loading) => {
                            if down {
                                loading.menu_key_focus(event_loop, key);
                            }
                        }
                    }
                }
                Input::Mouse(button, down) => {
                    self.window_event_here(
                        event_loop, id,
                        WindowEvent::MouseInput {
                            device_id: DeviceId::dummy(),
                            state: if down { ElementState::Pressed } else { ElementState::Released },
                            button: if button == 1 { MouseButton::Right } else { MouseButton::Left },
                        },
                    );
                }
                Input::Cursor(x, y) => self.window_event_here(
                    event_loop, id,
                    WindowEvent::CursorMoved {
                        device_id: DeviceId::dummy(),
                        position: winit::dpi::PhysicalPosition::new(x, y),
                    },
                ),
                Input::Motion(x, y) => {
                    if let App::Ready(game) = self {
                        let host = &mut game.host;
                        if host.screen == Screen::Playing
                            && host.title.is_none()
                            && !host.screen_over_playing()
                            && !host.scripted_run()
                            && host.pending_save.is_none()
                            && !host.pad_radial_captures()
                        {
                            // UIKit has no desktop mouse grab. Once the
                            // world is playing, use the iOS software capture.
                            if !host.captured {
                                host.capture(true);
                            }
                            if host.captured {
                                host.mouse.add_motion(x, y);
                                host.last_input = Instant::now();
                            }
                        }
                    }
                },
                Input::Text(text) => self.window_event_here(
                    event_loop, id,
                    WindowEvent::Ime(winit::event::Ime::Commit(text)),
                ),
                Input::KeyboardClosed => {
                    match self {
                        App::Loading(loading) => {
                            if let Some(window) = &loading.window {
                                window.set_ime_allowed(false);
                            }
                        }
                        App::Ready(game) => {
                            if let Some(window) = &game.host.window {
                                window.set_ime_allowed(false);
                            }
                        }
                    }
                }
                Input::Close => self.window_event_here(
                    event_loop, id, WindowEvent::CloseRequested,
                ),
            }
        }
    }

    /// Primary-finger touch -> existing cursor/mouse input. All events travel
    /// through Horizon's original menu/game handlers. Multi-touch contacts
    /// never steal the active finger or leave a held mouse button behind.
    #[cfg(target_os = "ios")]
    fn ios_touch_mouse(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        touch: &winit::event::Touch,
    ) {
        use std::sync::atomic::{AtomicU64, Ordering};
        use winit::event::TouchPhase;
        const EMPTY: u64 = u64::MAX;
        static ACTIVE: AtomicU64 = AtomicU64::new(EMPTY);

        let accepted = match touch.phase {
            TouchPhase::Started => ACTIVE
                .compare_exchange(EMPTY, touch.id, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
            TouchPhase::Moved => ACTIVE.load(Ordering::SeqCst) == touch.id,
            TouchPhase::Ended | TouchPhase::Cancelled => ACTIVE
                .compare_exchange(touch.id, EMPTY, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
        };
        if !accepted {
            return;
        }
        self.window_event_here(
            event_loop,
            id,
            WindowEvent::CursorMoved {
                device_id: DeviceId::dummy(),
                position: touch.location,
            },
        );
        let state = match touch.phase {
            TouchPhase::Started => Some(ElementState::Pressed),
            TouchPhase::Ended | TouchPhase::Cancelled => Some(ElementState::Released),
            TouchPhase::Moved => None,
        };
        if let Some(state) = state {
            self.window_event_here(
                event_loop,
                id,
                WindowEvent::MouseInput {
                    device_id: DeviceId::dummy(),
                    state,
                    button: MouseButton::Left,
                },
            );
        }
    }
    fn resumed_here(&mut self, event_loop: &ActiveEventLoop) {
        match self {
            App::Loading(loading) => loading.resumed(event_loop),
            App::Ready(game) => game.resumed(event_loop),
        }
    }
    fn window_event_here(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        #[cfg(target_os = "ios")]
        if let WindowEvent::Touch(touch) = &event {
            if horizon_client_input::ios_input::touch_gestures()
                && !horizon_client_input::ios_input::overlay_active()
            {
                self.ios_touch_mouse(event_loop, id, touch);
            }
            return;
        }
        match self {
            App::Loading(loading) => loading.window_event(event_loop, id, event),
            App::Ready(game) => game.window_event(event_loop, id, event),
        }
    }
    fn device_event_here(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        if let App::Ready(game) = self {
            game.device_event(event_loop, id, event);
        }
    }
    fn about_to_wait_here(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "ios")]
        self.ios_drain_inputs(event_loop);
        enum Step {
            Waiting,
            Ready(Box<split::Split<'static>>),
            Done,
            Failed(Box<dyn std::error::Error>),
        }
        let step = match self {
            App::Ready(game) => {
                game.about_to_wait(event_loop);
                return;
            }
            App::Loading(loading) => {
                loading.about_to_wait(event_loop);
                if loading.error.is_some() {
                    event_loop.exit();
                    return;
                }
                // Keep the iOS title/loading screen responsive while the
                // world-rest loader finishes. Joining an unfinished loader on
                // UIKit's main thread would freeze the event loop.
                #[cfg(target_os = "ios")]
                let rest_pending = loading.loaded.as_ref()
                    .and_then(|result| result.as_ref().ok())
                    .and_then(|loaded| loaded.rest_loader.as_ref())
                    .is_some_and(|worker| !worker.handle.is_finished());
                #[cfg(not(target_os = "ios"))]
                let rest_pending = false;
                match if loading.menu.reload_packs || rest_pending || !loading.ready_for_handoff() {
                    None
                } else {
                    loading.loaded.take()
                } {
                    None => Step::Waiting,
                    Some(Err(e)) => Step::Failed(e.into()),
                    Some(Ok(loaded)) => match loading.finish(loaded) {
                        Ok(Some(game)) => Step::Ready(Box::new(game)),
                        Ok(None) => Step::Done,
                        Err(e) => Step::Failed(e),
                    },
                }
            }
        };
        match step {
            Step::Waiting => {}
            Step::Ready(game) => {
                #[cfg(target_os = "ios")]
                NOVA_IOS_PHASE.store(2, std::sync::atomic::Ordering::Relaxed);
                eprintln!("[nova-watchdog] handoff.game-ready");
                *self = App::Ready(game);
            },
            Step::Done => event_loop.exit(),
            Step::Failed(e) => {
                if let App::Loading(loading) = self {
                    loading.error = Some(e);
                }
                event_loop.exit();
            }
        }
    }
    fn exiting_here(&mut self, event_loop: &ActiveEventLoop) {
        if let App::Ready(game) = self {
            game.exiting(event_loop);
        }
    }
}

/// Every launch reads the prepared game files (`horizon-prepare`'s output, whose
/// manifest is `horizon-resources.json`): without them, say where they were
/// looked for and how to get them, before a window or a loader starts. `get` is
/// the page a hook sends the player to (`horizon_hooks::engine::get_the_game`).
pub(super) fn missing_resources(
    root: &std::path::Path,
    get: Option<horizon_hooks::GetTheGame>,
) -> Option<String> {
    let shown = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());
    if !root.join("horizon-resources.json").is_file() {
        let launcher = get.map_or(String::new(), |get| {
            format!(
                "Horizon is started from its launcher: download it at {} (Download). It needs \
                 your own Minecraft: Java Edition; the launcher signs you in and prepares the \
                 game files.\nDevelopers: ",
                get.address
            )
        });
        return Some(format!(
            "Horizon's prepared game files are missing: no horizon-resources.json in {}\n\
             {launcher}horizon-prepare --prepare, or pass --resources <folder>",
            shown.display(),
        ));
    }
    damaged_prepared(root).map(|problem| {
        format!(
            "Horizon's prepared game files are damaged: {problem} in {}\n\
             Prepare them again with the launcher, or: horizon-prepare --prepare",
            shown.display()
        )
    })
}

/// Which top-level prepared file a killed preparation or an interrupted copy
/// left unusable. The loader parses these before it can start; naming one
/// beats serde's bare "expected value at line 1 column 1".
pub(super) fn damaged_prepared(root: &std::path::Path) -> Option<String> {
    let manifest = root.join("horizon-resources.json");
    let bytes = std::fs::read(&manifest).ok()?;
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Some("horizon-resources.json is empty".into());
    }
    if serde_json::from_slice::<serde_json::Value>(&bytes).is_err() {
        return Some("horizon-resources.json is not valid JSON".into());
    }
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.extension() != Some(std::ffi::OsStr::new("json")) {
            continue;
        }
        if std::fs::metadata(&path).is_ok_and(|meta| meta.len() == 0) {
            return Some(format!("{} is empty", entry.file_name().to_string_lossy()));
        }
    }
    None
}

/// `Minecraft.<init>`: `startedCleanly` left false means the previous start
/// died before `onGameLoadFinished`. Vanilla then leaves fullscreen
/// ("resetting fullscreen mode") so a start that died on the display mode can
/// come back; the resource packs are rolled back only when their load fails
/// (`rollbackResourcePacks`), not here. True when the last start was unclean.
pub(super) fn unclean_start_recovery(settings: &mut Settings) -> bool {
    // Vanilla's field defaults true: only an explicit `false` is an unclean
    // start; an options file that never had the key is not one.
    let unclean = matches!(
        settings.file.get("startedCleanly"),
        Some(horizon_client::options_file::V::Bool(false))
    );
    if unclean {
        settings.fullscreen = false;
        settings.file.fullscreen_resolution = None;
    }
    unclean
}
