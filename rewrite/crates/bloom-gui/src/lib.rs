mod font;
mod iced_canvas;
mod paint;
mod recorder;

pub use font::{
    editor_font_bytes, load_editor_font, CosmicFontMetrics, FontSpec, EDITOR_FONT_FAMILY,
    EDITOR_FONT_SIZE, EDITOR_LINE_HEIGHT,
};
pub use iced_canvas::IcedCanvasProgram;
pub use paint::{paint, PaintSink, PaintSnapshot, PaintedGrapheme};
pub use recorder::{
    EffectOutcome, EventKind, MetadataRecord, MetadataRecorder, RecordResult, RecorderError,
    RecorderLimits, RecorderStats, MAX_BYTES, MAX_RECORDS,
};
