use std::collections::HashMap;

use crate::input::KeyEvent;

/// Macro recording and playback state.
pub struct MacroState {
    recording: Option<char>,
    current_keys: Vec<KeyEvent>,
    registers: HashMap<char, Vec<KeyEvent>>,
}

impl MacroState {
    pub fn new() -> Self {
        Self {
            recording: None,
            current_keys: Vec::new(),
            registers: HashMap::new(),
        }
    }

    pub fn start_recording(&mut self, register: char) {
        self.recording = Some(register);
        self.current_keys.clear();
    }

    pub fn stop_recording(&mut self) {
        if let Some(reg) = self.recording.take() {
            self.registers
                .insert(reg, std::mem::take(&mut self.current_keys));
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    pub fn record_key(&mut self, key: KeyEvent) {
        if self.recording.is_some() {
            self.current_keys.push(key);
        }
    }

    pub fn get(&self, register: char) -> Vec<KeyEvent> {
        self.registers.get(&register).cloned().unwrap_or_default()
    }
}
