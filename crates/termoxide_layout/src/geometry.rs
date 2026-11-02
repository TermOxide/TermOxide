//! Cell rectangles, the border box model and border glyphs (ADR D5.5, D5.6).

use crate::style::{BorderSides, BorderStyle, Property, Style, Unit};

/// A rectangle in absolute terminal cells. Arithmetic saturates at the edges
/// of the `u16` grid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl CellRect {
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self { Self { x, y, width, height } }

    pub const fn right(self) -> u16 { self.x.saturating_add(self.width) }

    pub const fn bottom(self) -> u16 { self.y.saturating_add(self.height) }

    pub const fn is_empty(self) -> bool { self.width == 0 || self.height == 0 }

    /// The overlap of two rectangles; empty, at the nearer corner, when they
    /// do not meet.
    pub fn intersection(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Self {
            x,
            y,
            width: self.right().min(other.right()).saturating_sub(x),
            height: self.bottom().min(other.bottom()).saturating_sub(y),
        }
    }

    /// Shrinks the rectangle by the given edges, never below zero size.
    pub fn shrink(self, top: u16, right: u16, bottom: u16, left: u16) -> Self {
        let width = self.width.saturating_sub(left.saturating_add(right));
        let height = self.height.saturating_sub(top.saturating_add(bottom));
        Self {
            x: self.x.saturating_add(left.min(self.width)),
            y: self.y.saturating_add(top.min(self.height)),
            width,
            height,
        }
    }
}

/// The padding box of a node whose border box is `rect`: each side drawn in
/// `border_sides` takes one cell, whatever the `border_style` (ADR D5.6).
pub fn padding_box(style: &Style, rect: CellRect) -> CellRect {
    let sides = style.border_sides.value().unwrap_or(BorderSides::NONE);
    let side = |s| u16::from(sides.contains(s));
    rect.shrink(
        side(BorderSides::TOP),
        side(BorderSides::RIGHT),
        side(BorderSides::BOTTOM),
        side(BorderSides::LEFT),
    )
}

/// The content box of a node whose border box is `rect` (ADR D5.5).
///
/// Only `Cells` padding is subtracted; `Percent` padding is unresolved
/// before layout and counts as zero here, as does negative padding.
pub fn content_box(style: &Style, rect: CellRect) -> CellRect {
    let cells = |p: Property<Unit>| p.value().and_then(Unit::cells).map_or(0, |n| u16::try_from(n).unwrap_or(0));
    padding_box(style, rect).shrink(
        cells(style.padding_top),
        cells(style.padding_right),
        cells(style.padding_bottom),
        cells(style.padding_left),
    )
}

/// The glyphs of one border style (ADR D5.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorderGlyphs {
    pub top_left: char,
    pub horizontal: char,
    pub top_right: char,
    pub vertical: char,
    pub bottom_left: char,
    pub bottom_right: char,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

impl BorderGlyphs {
    const fn of(glyphs: [char; 6]) -> Self {
        let [top_left, horizontal, top_right, vertical, bottom_left, bottom_right] = glyphs;
        Self { top_left, horizontal, top_right, vertical, bottom_left, bottom_right }
    }

    /// The glyph at `corner` when `sides` are drawn. A corner glyph needs both
    /// adjacent sides; with one, that side's straight glyph runs into the
    /// corner; with none, nothing is drawn.
    pub const fn corner(self, corner: Corner, sides: BorderSides) -> Option<char> {
        let (horizontal, vertical, glyph) = match corner {
            Corner::TopLeft => (BorderSides::TOP, BorderSides::LEFT, self.top_left),
            Corner::TopRight => (BorderSides::TOP, BorderSides::RIGHT, self.top_right),
            Corner::BottomRight => (BorderSides::BOTTOM, BorderSides::RIGHT, self.bottom_right),
            Corner::BottomLeft => (BorderSides::BOTTOM, BorderSides::LEFT, self.bottom_left),
        };
        match (sides.contains(horizontal), sides.contains(vertical)) {
            (true, true) => Some(glyph),
            (true, false) => Some(self.horizontal),
            (false, true) => Some(self.vertical),
            (false, false) => None,
        }
    }
}

impl BorderStyle {
    pub const fn glyphs(self) -> BorderGlyphs {
        BorderGlyphs::of(match self {
            BorderStyle::Single => ['┌', '─', '┐', '│', '└', '┘'],
            BorderStyle::Double => ['╔', '═', '╗', '║', '╚', '╝'],
            BorderStyle::Rounded => ['╭', '─', '╮', '│', '╰', '╯'],
            BorderStyle::Thick => ['┏', '━', '┓', '┃', '┗', '┛'],
            BorderStyle::Ascii => ['+', '-', '+', '|', '+', '+'],
            BorderStyle::None => [' '; 6],
        })
    }
}
