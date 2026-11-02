pub mod coordinates;
mod doc;
mod projection;

pub use coordinates::VisiblePosition;
pub use doc::{Doc, DocError, DocSnapshot, EditDelta, GraphemeBoundaryBias};
pub use projection::{
    BlockIdMarker, BlockIdToken, LoweredEdit, ProjectionError, TokenDisposition, TokenEffect,
    VisibleProjection,
};
