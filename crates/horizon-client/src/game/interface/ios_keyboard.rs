// game-module-cfg: any(target_os = "ios", test)
//! Nova's iOS-only physical-key adapter.
//! UIKit controls and the software keyboard share this path. The native winit
//! KeyEvent type is intentionally never fabricated: its platform fields are private.
use super::*;
use horizon_client::debug_keys::KeyAction;

impl Game<'_> {
    pub(super) fn ios_virtual_key(
        &mut self,
        event_loop: &dyn EventLoopCtl,
        key: KeyCode,
        pressed: bool,
    ) {
        self.with_user_input(true, |game| {
            if let Err(error) = game.ios_virtual_key_inner(event_loop, key, pressed) {
                log::warn!("Nova iOS input: {key:?} ({pressed}): {error}");
            }
            game.settle_title_mode();
            let action = if pressed {
                mod_api::KeyAction::Press
            } else {
                mod_api::KeyAction::Release
            };
            game.virtual_key_input(key, action);
        });
    }

    fn ios_virtual_key_inner(
        &mut self,
        event_loop: &dyn EventLoopCtl,
        key: KeyCode,
        pressed: bool,
    ) -> Result<()> {
        if pressed {
            self.physical_keys.insert(key);
            self.input_keyboard_used();
            self.last_input = Instant::now();
        } else {
            self.physical_keys.remove(&key);
        }

        // winit normally emits both KeyboardInput and ModifiersChanged.
        // Nova's touch keys have no OS modifier callback, so update Horizon's
        // modifier word as well as its physical-key set. Sneak and sprint
        // check this word every frame and otherwise cancel a held Shift/Ctrl.
        // Reuse the upstream helper to preserve other held modifier keys.
        if matches!(
            key,
            KeyCode::ShiftLeft | KeyCode::ShiftRight
                | KeyCode::ControlLeft | KeyCode::ControlRight
                | KeyCode::AltLeft | KeyCode::AltRight
                | KeyCode::SuperLeft | KeyCode::SuperRight
        ) {
            horizon_client_input::modifiers::set_key(
                &mut self.modifiers, &mut self.physical_keys, key, pressed,
            );
            self.update_modifiers(self.modifiers);
        }

        // Desktop keyboard events reach debug handling before screens and
        // bindings; F3 toggles on release, so both edges must be forwarded.
        let action = if pressed { KeyAction::Press } else { KeyAction::Release };
        if self.debug_key(key, action) {
            return Ok(());
        }
        if !pressed {
            if self.screen_key_released(key) {
                self.keys.remove(&key);
                return self.mapped_key_event(key, false, false);
            }
            if self.screen == Screen::Options {
                self.ui.options.modifiers = self.modifiers().bits;
                let events = self.ui.options.key_released(&mut self.settings, key);
                self.handle_option_events(events)?;
            }
            if self.screen == Screen::Chat && self.title.is_none() {
                self.keys.remove(&key);
            }
            return self.unhandled_key(key, false, false);
        }

        if self.screen_key_pressed(key, false) {
            return Ok(());
        }
        if self.key_focus_key(event_loop, key) {
            return Ok(());
        }
        if self.dialog_open() {
            return self.dialog_key(key, None, None);
        }
        if self.screen == Screen::Chat && self.title.is_none() {
            self.keys.insert(key);
            self.chat_key_code(key, None);
            return Ok(());
        }
        if self.screen == Screen::Options {
            return self.options_key(key, None);
        }
        if self.title_input_active() {
            let (w, h) = self.gui_size();
            match key {
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    let (action, sound) = self
                        .title
                        .as_mut()
                        .map_or((Action::None, false), |t| t.confirm_key_size((w, h)));
                    self.save_title_options();
                    if sound {
                        self.audio.play_button_click();
                    }
                    self.title_action(event_loop, action)?;
                }
                KeyCode::Backspace | KeyCode::Delete
                | KeyCode::ArrowLeft | KeyCode::ArrowRight
                | KeyCode::Home | KeyCode::End
                    if self.title.as_mut().is_some_and(|t| t.has_caret_field()) =>
                {
                    let (ctrl, shift) = (self.has_control_down(), self.has_shift_down());
                    if let Some(t) = self.title.as_mut() {
                        t.field_key(key, ctrl, shift);
                    }
                }
                KeyCode::ArrowLeft | KeyCode::ArrowRight
                | KeyCode::ArrowUp | KeyCode::ArrowDown => {
                    let shift = self.has_shift_down();
                    if let Some(t) = self.title.as_mut() {
                        t.arrow_key(key, (w, h), shift);
                    }
                }
                KeyCode::Backspace => self.title_edit(None, true),
                KeyCode::Tab => {
                    let shift = self.has_shift_down();
                    if let Some(t) = self.title.as_mut() {
                        t.tab_dir(shift, (w, h));
                    }
                }
                KeyCode::Escape => {
                    if let Some(t) = self.title.as_mut() {
                        t.escape();
                        self.reopen_data_packs(event_loop)?;
                    }
                    self.save_title_options();
                }
                KeyCode::F5 => {
                    if let Some(t) = self.title.as_mut() {
                        t.refresh_servers();
                    }
                }
                // The IME commit path delivers printable text separately.
                _ => {}
            }
            return Ok(());
        }

        // Text fields hosted inside screens also consume their own raw keys.
        if self.station_typing() && key != KeyCode::Escape {
            if !self.station_key(key) {
                self.station_text(None, key == KeyCode::Backspace);
            }
            return Ok(());
        }
        if self.screen == Screen::Creative
            && self.creative_screen.tab().search
            && !self.mod_browser_typing()
            && key != KeyCode::Escape
        {
            if !self.search_tab_hotbar_key(key) {
                let before = self.creative_screen.search.value.clone();
                let (ctrl, shift) = (self.has_control_down(), self.has_shift_down());
                if self.creative_screen.search.key(key, ctrl, shift)
                    && self.creative_screen.search.value != before
                {
                    self.creative_screen.scroll = 0.0;
                }
            }
            return Ok(());
        }
        if self.screen == Screen::Paused && self.pause_menu && key != KeyCode::Escape {
            self.pause_key(key)?;
            return Ok(());
        }

        self.unhandled_key(key, true, false)
    }
}
