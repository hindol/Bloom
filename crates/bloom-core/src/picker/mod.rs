//! Fuzzy picker with [`nucleo`]-powered matching.
//!
//! [`Picker<T>`] manages a list of items, applies fuzzy or word-based filtering
//! as the user types, and supports multi-selection (marking). Used for page
//! search, buffer switching, tag selection, template choice, and more.

pub mod filter;
pub mod nucleo;
#[allow(clippy::module_inception)]
pub mod picker;
pub mod source;

pub use filter::PickerFilter;
pub use picker::{MatchMode, Picker};
pub use source::{ColumnStyle, PickerColumn, PickerItem, PickerRow};
