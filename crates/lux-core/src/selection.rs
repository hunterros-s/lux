//! Selection mode types.

use serde::{Deserialize, Serialize};

/// Selection mode for a view.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SelectionMode {
    /// Selecting an item clears previous selection.
    #[default]
    Single,
    /// Selecting toggles. Multiple items can be selected.
    Multi,
    /// `on_select` hook controls all selection logic.
    Custom,
}
