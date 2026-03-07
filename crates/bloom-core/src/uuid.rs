use crate::types::PageId;
use uuid::Uuid;

/// Generate a random 8-char hex string (4 bytes).
pub fn generate_hex_id() -> PageId {
    let uuid = Uuid::new_v4();
    let bytes = uuid.as_bytes();
    PageId([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Check if an ID collides with existing pages.
pub fn is_unique(id: &PageId, existing_ids: &[PageId]) -> bool {
    !existing_ids.contains(id)
}

/// Generate a unique ID, retrying on collision.
pub fn generate_unique_id(existing_ids: &[PageId]) -> PageId {
    loop {
        let id = generate_hex_id();
        if is_unique(&id, existing_ids) {
            return id;
        }
    }
}
