//! Xterm.js related logic for terminal manipulation.

use crate::Result;

use xterm_js_sys::xterm::Terminal;

// pub(crate) fn is_raw_mode_enabled() -> bool { true }

/// Returns the terminal size `(columns, rows)`.
///
/// The top left cell is represented `(1, 1)`.
pub fn size(term: &Terminal) -> Result<(u16, u16)> {
    Ok((term.cols(), term.rows()))
}

// pub(crate) fn enable_raw_mode() -> Result<()> { Ok(()) }

// pub(crate) fn disable_raw_mode() -> Result<()> { Ok(()) }
