//! Item and Group types for search results.

use serde::{Deserialize, Serialize};
use std::hash::Hash;

/// Stable item identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemId(pub String);

impl From<String> for ItemId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ItemId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for ItemId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// An item is the atomic unit of data in Lux.
///
/// Everything users search, select, and act upon is an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    /// Unique identifier within the current result set.
    pub id: String,

    /// Primary display text.
    pub title: String,

    /// Secondary display text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,

    /// Icon identifier (path, emoji, or named icon).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// Array of type tags for action filtering.
    /// E.g., ["file", "typescript", "react"]
    #[serde(default)]
    pub types: Vec<String>,

    /// Arbitrary data for actions to consume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Item {
    /// Create a new item with required fields.
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            icon: None,
            types: Vec::new(),
            data: None,
        }
    }

    /// Check if this item has a specific type tag.
    pub fn has_type(&self, type_name: &str) -> bool {
        self.types.iter().any(|t| t == type_name)
    }

    /// Get the item's ID as an ItemId.
    pub fn item_id(&self) -> ItemId {
        ItemId(self.id.clone())
    }
}

/// A group of items with an optional title.
///
/// Sources return groups to enable sectioned results like
/// "Recent", "Suggested", "All Files", etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// Optional section title. If None, items are ungrouped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Items in this group.
    pub items: Vec<Item>,
}

impl Group {
    /// Create a new group with a title.
    pub fn new(title: impl Into<String>, items: Vec<Item>) -> Self {
        Self {
            title: Some(title.into()),
            items,
        }
    }

    /// Create an ungrouped group (no title).
    pub fn ungrouped(items: Vec<Item>) -> Self {
        Self { title: None, items }
    }

    /// Check if the group is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get the number of items in the group.
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

/// A collection of groups returned by sources.
pub type Groups = Vec<Group>;
