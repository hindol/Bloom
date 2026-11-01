//! Opaque text-coordinate units.

use std::sync::Arc;

macro_rules! define_index {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(usize);
    };
}

/// A Unicode scalar-value offset in canonical rope text.
///
/// It is distinct from a byte or grapheme offset:
///
/// ```compile_fail
/// use bloom_text::coordinates::{ByteIdx, RawCharIdx};
///
/// fn needs_char(_: RawCharIdx) {}
/// fn pass_bytes(index: ByteIdx) {
///     needs_char(index);
/// }
/// ```
///
/// Raw construction is reserved for `Doc`, which owns coordinate conversion:
///
/// ```compile_fail
/// use bloom_text::coordinates::RawCharIdx;
///
/// let _ = RawCharIdx(3);
/// ```
///
/// There is no public conversion from an untyped integer:
///
/// ```compile_fail
/// use bloom_text::coordinates::RawCharIdx;
///
/// let _: RawCharIdx = 3usize.into();
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RawCharIdx(usize);

impl RawCharIdx {
    pub(crate) const fn new(value: usize) -> Self {
        Self(value)
    }

    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

/// A Unicode scalar-value offset in the visible projection.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VisibleCharIdx(usize);

impl VisibleCharIdx {
    pub(crate) const fn new(value: usize) -> Self {
        Self(value)
    }

    pub const fn get(self) -> usize {
        self.0
    }
}

/// A half-open range in canonical raw-text coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawCharRange {
    start: RawCharIdx,
    end: RawCharIdx,
}

impl RawCharRange {
    pub(crate) const fn new(start: RawCharIdx, end: RawCharIdx) -> Self {
        Self { start, end }
    }

    pub const fn start(self) -> RawCharIdx {
        self.start
    }

    pub const fn end(self) -> RawCharIdx {
        self.end
    }
}

/// A half-open, grapheme-aligned range tied to the projection that created it.
///
/// The range cannot be lowered against a different document snapshot.
#[derive(Clone, Debug)]
pub struct VisibleRange {
    start: VisibleCharIdx,
    end: VisibleCharIdx,
    projection_id: Arc<()>,
}

impl VisibleRange {
    pub(crate) fn new(start: VisibleCharIdx, end: VisibleCharIdx, projection_id: Arc<()>) -> Self {
        Self {
            start,
            end,
            projection_id,
        }
    }

    pub const fn start(&self) -> VisibleCharIdx {
        self.start
    }

    pub const fn end(&self) -> VisibleCharIdx {
        self.end
    }

    pub(crate) fn belongs_to(&self, projection_id: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.projection_id, projection_id)
    }
}

impl PartialEq for VisibleRange {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start
            && self.end == other.end
            && Arc::ptr_eq(&self.projection_id, &other.projection_id)
    }
}

impl Eq for VisibleRange {}

define_index!(ByteIdx, "A byte offset, valid only at a UTF-8 boundary.");
define_index!(LineIdx, "A zero-based line index.");
define_index!(GraphemeIdx, "A zero-based extended grapheme-cluster index.");
