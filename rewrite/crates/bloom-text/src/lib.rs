pub mod coordinates;
mod doc;
mod projection;

pub use doc::{Doc, DocError, DocSnapshot, EditDelta};
pub use projection::{
    BlockIdMarker, BlockIdToken, LoweredEdit, ProjectionError, TokenDisposition, TokenEffect,
    VisibleProjection,
};
