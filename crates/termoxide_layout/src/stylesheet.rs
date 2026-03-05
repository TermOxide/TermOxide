use std::collections::HashMap;

use oxidui_style::Style;

/// A small named registry for `oxidui_style::Style` values.
///
/// `StyleSheet` stores named `Style` entries so components can share
/// and reuse style declarations by name (themes, component presets, …).
#[derive(Debug, Clone)]
pub struct StyleSheet {
    map: HashMap<String, Style>,
}

impl StyleSheet {
    /// Create an empty stylesheet.
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Register or replace a named style.
    pub fn register(&mut self, name: impl Into<String>, style: Style) {
        self.map.insert(name.into(), style);
    }

    /// Get a style by name.
    pub fn get(&self, name: &str) -> Option<&Style> {
        self.map.get(name)
    }

    /// Merge a named style onto a base style and return the resulting
    /// merged style. If the named style does not exist, `base` is
    /// returned unchanged.
    pub fn merged_with(&self, name: &str, base: &Style) -> Style {
        if let Some(s) = self.map.get(name) {
            base.merged_with(s)
        } else {
            base.clone()
        }
    }
}
