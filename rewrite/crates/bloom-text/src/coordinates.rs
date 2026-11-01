//! Opaque text-coordinate units.

macro_rules! define_index {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(usize);
    };
}

/// A Unicode scalar-value offset.
///
/// It is distinct from a byte or grapheme offset:
///
/// ```compile_fail
/// use bloom_text::coordinates::{ByteIdx, CharIdx};
///
/// fn needs_char(_: CharIdx) {}
/// fn pass_bytes(index: ByteIdx) {
///     needs_char(index);
/// }
/// ```
///
/// Raw construction is reserved for `Doc`, which owns coordinate conversion:
///
/// ```compile_fail
/// use bloom_text::coordinates::CharIdx;
///
/// let _ = CharIdx(3);
/// ```
///
/// There is no public conversion from an untyped integer:
///
/// ```compile_fail
/// use bloom_text::coordinates::CharIdx;
///
/// let _: CharIdx = 3usize.into();
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CharIdx(usize);

impl CharIdx {
    pub(crate) const fn new(value: usize) -> Self {
        Self(value)
    }

    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

define_index!(ByteIdx, "A byte offset, valid only at a UTF-8 boundary.");
define_index!(LineIdx, "A zero-based line index.");
define_index!(GraphemeIdx, "A zero-based extended grapheme-cluster index.");
