use std::collections::HashMap;

/// Named register file supporting 'a'-'z', unnamed '"', and clipboard '+'.
pub struct RegisterFile {
    registers: HashMap<char, String>,
}

impl RegisterFile {
    pub fn new() -> Self {
        Self {
            registers: HashMap::new(),
        }
    }

    /// Get register contents. Lowercase name for named registers.
    pub fn get(&self, name: char) -> Option<&str> {
        let key = name.to_ascii_lowercase();
        self.registers.get(&key).map(|s| s.as_str())
    }

    /// Set register contents. Uppercase appends; lowercase overwrites.
    /// Also updates the unnamed register '"' for non-clipboard writes.
    pub fn set(&mut self, name: char, content: String) {
        if name.is_ascii_uppercase() {
            let lower = name.to_ascii_lowercase();
            self.registers
                .entry(lower)
                .or_default()
                .push_str(&content);
        } else {
            self.registers.insert(name, content.clone());
        }
        // Update unnamed register for any write
        if name != '"' {
            self.registers.insert('"', content);
        }
    }
}