// game-module-cfg: target_os = "ios"
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
        if let Err(error) = self.ios_virtual_key_inner(event_loop, key, pressed) {
            log::warn!("Nova iOS input: {key:?} ({pressed}): {error}");
        }
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
            if self.screen == Screen::Chat && self.title.is_none() {
                self.keys.remove(&key);
            }
            return self.mapped_key_event(key, false, false);
        }

        // Vanilla uses raw keys for the active screen before translating a
        // key into movement or another mapped gameplay action.
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

        // Movement, jump, inventory and other gameplay mappings remain the
        // original Horizon implementation. Don't alter camera or touch input.
        self.mapped_key_event(key, true, false)
    }
}
