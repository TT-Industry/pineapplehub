//! Material Symbols icon font integration.
//!
//! Provides a subset of Material Symbols Outlined as an embedded font,
//! with constants for each icon codepoint used in the app.
//!
//! ## Adding new icons
//!
//! 1. Look up the codepoint in the MaterialSymbolsOutlined `.codepoints` file.
//! 2. Add a `pub(crate) const ICON_…` constant below.
//! 3. Regenerate the font subset — see `assets/README.md` for the full command.

use iced::widget::text;
use iced::{Element, Font};

/// The Material Symbols icon font, loaded from the embedded subset.
pub(crate) const ICON_FONT: Font = Font::with_name("Material Symbols Outlined");

/// Font bytes for registration via `iced::Settings::fonts`.
pub(crate) const ICON_FONT_BYTES: &[u8] = include_bytes!("../assets/material-symbols.ttf");

// ──────────────────────── Icon Codepoints ────────────────────────
// Codepoints are from MaterialSymbolsOutlined variable font,
// NOT the legacy Material Icons font (which uses different codepoints).
//
// Look up: github.com/google/material-design-icons → variablefont/ → .codepoints file

pub(crate) const ICON_STAR: &str = "\u{F09A}";         // star (filled via button style)
pub(crate) const ICON_STAR_BORDER: &str = "\u{F09A}";  // same glyph, unstarred via styling
pub(crate) const ICON_EDIT: &str = "\u{F097}";          // edit
pub(crate) const ICON_COMMENT: &str = "\u{E0CB}";       // chat_bubble_outline (no note)
pub(crate) const ICON_COMMENT_FILLED: &str = "\u{F18B}"; // mark_chat_read (has note)
pub(crate) const ICON_WARNING: &str = "\u{F083}";       // warning
pub(crate) const ICON_DELETE: &str = "\u{E92E}";        // delete
pub(crate) const ICON_DOWNLOAD: &str = "\u{F090}";      // download
pub(crate) const ICON_BAR_CHART: &str = "\u{E26B}";    // bar_chart
pub(crate) const ICON_DESCRIPTION: &str = "\u{E873}";  // description
pub(crate) const ICON_HELP: &str = "\u{E8FD}";          // help_outline
pub(crate) const ICON_HISTORY: &str = "\u{E8B3}";       // history
pub(crate) const ICON_CLEANING: &str = "\u{F0FF}";      // cleaning_services
pub(crate) const ICON_SELECT_ALL: &str = "\u{E162}";   // select_all
pub(crate) const ICON_CLOSE: &str = "\u{E5CD}";         // close
pub(crate) const ICON_UNDO: &str = "\u{E166}";          // undo
pub(crate) const ICON_INFO: &str = "\u{E88E}";          // info
pub(crate) const ICON_MORE_VERT: &str = "\u{E5D4}";    // more_vert
pub(crate) const ICON_SEARCH: &str = "\u{E8B6}";        // search
pub(crate) const ICON_CHECK_CIRCLE: &str = "\u{F0BE}"; // check_circle
pub(crate) const ICON_CANCEL: &str = "\u{E888}";        // cancel
pub(crate) const ICON_ARROW_UPWARD: &str = "\u{E5D8}";  // arrow_upward
pub(crate) const ICON_ARROW_DOWNWARD: &str = "\u{E5DB}"; // arrow_downward
pub(crate) const ICON_UNFOLD_MORE: &str = "\u{E5D7}";   // unfold_more (sortable hint)
pub(crate) const ICON_HOURGLASS: &str = "\u{E88B}";      // hourglass_empty (queued)
pub(crate) const ICON_HOURGLASS_TOP: &str = "\u{EA5B}";  // hourglass_top (decoding)
pub(crate) const ICON_SYNC: &str = "\u{E627}";           // sync (processing)
pub(crate) const ICON_ERROR: &str = "\u{E000}";          // error
pub(crate) const ICON_CHEVRON_LEFT: &str = "\u{E5CB}";    // chevron_left
pub(crate) const ICON_CHEVRON_RIGHT: &str = "\u{E5CC}";   // chevron_right

/// Helper: create an icon text element with the given codepoint and size.
pub(crate) fn icon<'a, Message: 'a>(codepoint: &'a str, size: f32) -> Element<'a, Message> {
    text(codepoint)
        .font(ICON_FONT)
        .size(size)
        .into()
}
