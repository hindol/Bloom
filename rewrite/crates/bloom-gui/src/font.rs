use std::fmt;
use std::{collections::HashMap, sync::Mutex};

use bloom_kernel::geometry::{FontMetrics, PositivePx, Px};
use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

pub const EDITOR_FONT_FAMILY: &str = "JetBrains Mono";
pub const EDITOR_FONT_SIZE: f32 = 13.0;
pub const EDITOR_LINE_HEIGHT: f32 = 19.5;
const MAX_CACHED_ADVANCES: usize = 512;
const MAX_CACHED_GRAPHEME_BYTES: usize = 256;

const EDITOR_FONT_BYTES: &[u8] =
    include_bytes!("../../../../crates/bloom-gui/fonts/JetBrainsMono-Regular.ttf");

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontSpec {
    family: &'static str,
    size: Px,
    line_height: PositivePx,
}

impl FontSpec {
    pub fn editor_default() -> Self {
        Self {
            family: EDITOR_FONT_FAMILY,
            size: Px::new(EDITOR_FONT_SIZE).expect("editor font size is finite and positive"),
            line_height: PositivePx::new(
                Px::new(EDITOR_LINE_HEIGHT).expect("editor line height is finite and positive"),
            )
            .expect("editor line height is positive"),
        }
    }

    pub const fn family(self) -> &'static str {
        self.family
    }

    pub const fn size(self) -> Px {
        self.size
    }

    pub const fn line_height(self) -> PositivePx {
        self.line_height
    }
}

struct CosmicState {
    font_system: FontSystem,
    advances: HashMap<String, Px>,
}

pub struct CosmicFontMetrics {
    spec: FontSpec,
    state: Mutex<CosmicState>,
}

impl fmt::Debug for CosmicFontMetrics {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CosmicFontMetrics")
            .field("spec", &self.spec)
            .finish_non_exhaustive()
    }
}

impl CosmicFontMetrics {
    pub fn new(spec: FontSpec) -> Self {
        let mut font_system = FontSystem::new();
        font_system
            .db_mut()
            .load_font_data(EDITOR_FONT_BYTES.to_vec());

        Self {
            spec,
            state: Mutex::new(CosmicState {
                font_system,
                advances: HashMap::new(),
            }),
        }
    }

    pub const fn spec(&self) -> FontSpec {
        self.spec
    }
}

impl Default for CosmicFontMetrics {
    fn default() -> Self {
        Self::new(FontSpec::editor_default())
    }
}

impl FontMetrics for CosmicFontMetrics {
    fn grapheme_advance(&self, grapheme: &str) -> Px {
        if grapheme.is_empty() {
            return Px::ZERO;
        }

        let mut state = self
            .state
            .lock()
            .expect("cosmic-text font system lock must not be poisoned");
        if let Some(advance) = state.advances.get(grapheme) {
            return *advance;
        }

        let font_system = &mut state.font_system;
        let mut buffer = Buffer::new(
            font_system,
            Metrics::new(self.spec.size().get(), self.spec.line_height().get()),
        );
        buffer.set_size(
            font_system,
            Some(100_000.0),
            Some(self.spec.line_height().get()),
        );
        let attrs = Attrs::new().family(Family::Name(self.spec.family()));
        buffer.set_text(font_system, grapheme, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(font_system, false);

        let width = buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter())
            .map(|glyph| glyph.x + glyph.w)
            .fold(0.0_f32, f32::max);
        let advance = Px::new(width.max(0.0))
            .expect("cosmic-text grapheme advance must be finite and non-negative");
        if grapheme.len() <= MAX_CACHED_GRAPHEME_BYTES && state.advances.len() < MAX_CACHED_ADVANCES
        {
            state.advances.insert(grapheme.to_owned(), advance);
        }
        advance
    }

    fn line_height(&self) -> PositivePx {
        self.spec.line_height()
    }
}

pub fn editor_font_bytes() -> &'static [u8] {
    EDITOR_FONT_BYTES
}

pub fn load_editor_font() -> iced::Task<Result<(), iced::font::Error>> {
    iced::font::load(EDITOR_FONT_BYTES)
}

#[cfg(test)]
mod tests {
    use bloom_kernel::geometry::{FontMetrics, Px};

    use super::{editor_font_bytes, CosmicFontMetrics, FontSpec};

    #[test]
    fn cosmic_metrics_measure_with_the_bundled_editor_font() {
        let metrics = CosmicFontMetrics::new(FontSpec::editor_default());

        let narrow = metrics.grapheme_advance("i");
        let wide = metrics.grapheme_advance("W");

        assert!(narrow.get() > 0.0);
        assert_eq!(narrow, wide);
        assert!(!editor_font_bytes().is_empty());
        assert_eq!(metrics.grapheme_advance(""), Px::ZERO);
        metrics.grapheme_advance("private note");
        assert!(!format!("{metrics:?}").contains("private note"));
    }
}
