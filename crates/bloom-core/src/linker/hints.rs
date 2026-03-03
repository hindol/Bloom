use crate::index::Index;
use crate::types::PageId;

use super::resolver::{HintUpdate, Linker};

/// Convenience: compute all hint updates when a page is renamed.
pub fn compute_hint_updates(
    linker: &Linker,
    old_title: &str,
    new_title: &str,
    page_id: &PageId,
    index: &Index,
) -> Vec<HintUpdate> {
    linker.update_display_hints(old_title, new_title, page_id, index)
}