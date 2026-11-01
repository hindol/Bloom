//! Opaque units produced by the kernel's layout authority.

/// A zero-based display column produced by layout.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DisplayCol(usize);

/// A finite, non-negative logical-pixel measure.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Px(f32);
