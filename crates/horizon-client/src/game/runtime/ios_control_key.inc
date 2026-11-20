// Compiled into Horizon's existing control-command module on iOS only.
// This is an adapter, not a separate reimplementation of keyboard handling.
// No socket is opened: the input is dispatched on Horizon's event-loop thread.
impl Game<'_> {
    pub(super) fn ios_control_key(
        &mut self,
        event_loop: &dyn EventLoopCtl,
        key: KeyCode,
        pressed: bool,
    ) -> Result<()> {
        let Some(name) = horizon_client::keybinds::key_name(key) else {
            return Ok(());
        };
        let line = format!("key {name} {}", if pressed { "down" } else { "up" });
        let (reply, _receiver) = std::sync::mpsc::channel();
        // Reuse the control path's existing screen dispatch, key bindings,
        // modifier tracking, and mod input_key / consume_click handling.
        self.control_command(&line, &reply, event_loop)?;
        Ok(())
    }
}
