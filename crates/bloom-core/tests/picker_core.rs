pub mod document {
    pub use bloom_core::document::*;
}

pub mod index {
    pub use bloom_core::index::*;
}

pub mod parser {
    pub use bloom_core::parser::*;
}

pub mod render {
    pub use bloom_core::render::*;
}

pub mod resolver {
    pub use bloom_core::resolver::*;
}

pub mod template {
    pub use bloom_core::template::*;
}

#[path = "../src/picker.rs"]
mod picker;
