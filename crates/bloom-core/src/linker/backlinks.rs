use crate::index::{Backlink, Index};
use crate::types::PageId;

/// Retrieve backlinks to a given page from the index.
pub fn backlinks_for(page: &PageId, index: &Index) -> Vec<Backlink> {
    index.backlinks_to(page)
}
