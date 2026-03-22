use std::collections::{HashMap, VecDeque};

const KILL_RING_MAX: usize = 32;

/// Named register file supporting 'a'-'z', unnamed '"', and clipboard '+'.
/// Also maintains a kill ring (max 32 entries) of recent yank/delete contents.
pub struct RegisterFile {
    registers: HashMap<char, String>,
    kill_ring: VecDeque<String>,
}

impl RegisterFile {
    pub fn new() -> Self {
        Self {
            registers: HashMap::new(),
            kill_ring: VecDeque::new(),
        }
    }

    /// Get register contents. Lowercase name for named registers.
    pub fn get(&self, name: char) -> Option<&str> {
        let key = name.to_ascii_lowercase();
        self.registers.get(&key).map(|s| s.as_str())
    }

    /// Set register contents. Uppercase appends; lowercase overwrites.
    /// Also updates the unnamed register '"' for non-clipboard writes,
    /// and pushes to the kill ring (except for clipboard register '+').
    pub fn set(&mut self, name: char, content: String) {
        if name.is_ascii_uppercase() {
            let lower = name.to_ascii_lowercase();
            self.registers.entry(lower).or_default().push_str(&content);
        } else {
            self.registers.insert(name, content.clone());
        }
        // Update unnamed register for any write
        if name != '"' {
            self.registers.insert('"', content.clone());
        }
        // Push to kill ring (except clipboard register)
        if name != '+' {
            self.kill_ring.push_front(content);
            if self.kill_ring.len() > KILL_RING_MAX {
                self.kill_ring.pop_back();
            }
        }
    }

    /// The kill ring: most-recent-first history of yank/delete contents.
    pub fn kill_ring(&self) -> &VecDeque<String> {
        &self.kill_ring
    }
}
