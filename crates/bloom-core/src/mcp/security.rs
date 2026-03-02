#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    pub mode: AccessMode,
    pub exclude_patterns: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    ReadOnly,
    ReadWrite,
}

impl SecurityPolicy {
    pub fn allows_write(&self) -> bool {
        self.mode == AccessMode::ReadWrite
    }

    pub fn is_path_allowed(&self, path: &str) -> bool {
        for pattern in &self.exclude_patterns {
            if glob_match(pattern, path) {
                return false;
            }
        }
        true
    }
}

/// Simple glob matching with `*` wildcard support.
fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut pos = 0;

    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match text[pos..].find(part) {
            Some(found) => {
                // First segment must anchor at the start if pattern doesn't start with *
                if i == 0 && !pattern.starts_with('*') && found != 0 {
                    return false;
                }
                pos += found + part.len();
            }
            None => return false,
        }
    }

    // Last segment must anchor at the end if pattern doesn't end with *
    if !pattern.ends_with('*') && pos != text.len() {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_match_basics() {
        assert!(glob_match("*.md", "hello.md"));
        assert!(!glob_match("*.md", "hello.txt"));
        assert!(glob_match("private/*", "private/secret.md"));
        assert!(!glob_match("private/*", "public/readme.md"));
        assert!(glob_match("*secret*", "my-secret-file.md"));
        assert!(!glob_match("*secret*", "my-public-file.md"));
    }

    #[test]
    fn exact_match() {
        assert!(glob_match("hello.md", "hello.md"));
        assert!(!glob_match("hello.md", "world.md"));
    }
}
