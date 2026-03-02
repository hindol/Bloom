pub mod document {
    pub use bloom_core::document::*;
}

pub mod parser {
    pub use bloom_core::parser::*;
}

#[path = "../src/index.rs"]
mod index;
