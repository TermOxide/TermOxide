//! How a [`ViewNode`](crate::view_node::ViewNode)'s styles are serialized.
//!
//! ratatui's colours serialize as text and deserialize through an untagged
//! enum, which postcard and other non-self-describing formats can't decode, so
//! a style crosses with its colours as [`Color`]s. Underline colours are
//! dropped: ratatui only has them with its `underline-color` feature, which
//! needs `crossterm`.

use ratatui::style::{Modifier, Style};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use termoxide_layout::style::color::Color;

/// A style slot left unset inherits: it crosses as [`Color::Inherit`].
fn slot_to_wire(color: Option<ratatui::style::Color>) -> Color { color.map_or(Color::Inherit, Color::from) }

fn slot_from_wire(color: Color) -> Option<ratatui::style::Color> {
    (color != Color::Inherit).then(|| color.to_ratatui())
}

pub(crate) mod style {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(style: &Style, serializer: S) -> Result<S::Ok, S::Error> {
        (
            slot_to_wire(style.fg),
            slot_to_wire(style.bg),
            style.add_modifier,
            style.sub_modifier,
        )
            .serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Style, D::Error> {
        let (fg, bg, add_modifier, sub_modifier) = <(Color, Color, Modifier, Modifier)>::deserialize(deserializer)?;
        #[allow(
            clippy::needless_update,
            reason = "`Style` has an `underline_color` only when ratatui has `crossterm`"
        )]
        Ok(Style {
            fg: slot_from_wire(fg),
            bg: slot_from_wire(bg),
            add_modifier,
            sub_modifier,
            ..Style::default()
        })
    }
}
