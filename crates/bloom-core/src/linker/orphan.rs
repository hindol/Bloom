use crate::index::{Index, OrphanedLink};
use crate::types::PageId;

/// Retrieve orphaned (broken) links from a page.
pub fn orphaned_links_for(page: &PageId, index: &Index) -> Vec<OrphanedLink> {
    index.orphaned_links(page)
}
