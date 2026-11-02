use termoxide_layout::{
    paint::{ConcreteColor, PaintStyle, Palette, lower},
    style::{BorderStyle, Color, Property, Sem, Style, TextOverflow},
};

#[test]
fn default_palette_follows_the_terminal_theme() {
    let palette = Palette::default();
    let expected = [
        (Sem::Accent, 4),
        (Sem::Error, 1),
        (Sem::Warning, 3),
        (Sem::Surface, 0),
        (Sem::Muted, 8),
    ];
    for (sem, entry) in expected {
        assert_eq!(lower(Color::Semantic(sem), &palette), ConcreteColor::Base16(entry));
    }
}

#[test]
fn semantic_colours_go_through_the_palette() {
    let palette = Palette { accent: ConcreteColor::Rgb(0, 128, 255), ..Palette::default() };
    assert_eq!(lower(Color::Semantic(Sem::Accent), &palette), ConcreteColor::Rgb(0, 128, 255));
    assert_eq!(palette.get(Sem::Muted), ConcreteColor::Base16(8));
}

#[test]
fn concrete_colours_pass_through() {
    let palette = Palette::default();
    assert_eq!(lower(Color::Base16(12), &palette), ConcreteColor::Base16(12));
    assert_eq!(lower(Color::Rgb(9, 8, 7), &palette), ConcreteColor::Rgb(9, 8, 7));
}

#[test]
fn unset_paint_is_terminal_default_and_off() {
    assert_eq!(PaintStyle::new(&Style::new(), &Palette::default()), PaintStyle::default());
    let explicit_off = Style::new().bold(false);
    assert!(!PaintStyle::new(&explicit_off, &Palette::default()).bold);
}

#[test]
fn paint_style_lowers_every_paint_property() {
    let style = Style::new()
        .fg(Color::Semantic(Sem::Warning))
        .bg(Color::Rgb(1, 1, 1))
        .bold(true)
        .dim(true)
        .italic(true)
        .underline(true)
        .strikethrough(true)
        .reverse(true)
        .border_style(BorderStyle::Rounded)
        .text_overflow(TextOverflow::Ellipsis);
    assert_eq!(PaintStyle::new(&style, &Palette::default()), PaintStyle {
        fg: Some(ConcreteColor::Base16(3)),
        bg: Some(ConcreteColor::Rgb(1, 1, 1)),
        bold: true,
        dim: true,
        italic: true,
        underline: true,
        strikethrough: true,
        reverse: true,
        border_style: Some(BorderStyle::Rounded),
        text_overflow: Some(TextOverflow::Ellipsis),
    });
}

#[test]
fn inherit_left_in_a_style_is_treated_as_unset() {
    let style = Style { fg: Property::Inherit, italic: Property::Inherit, ..Style::new() };
    assert_eq!(PaintStyle::new(&style, &Palette::default()), PaintStyle::default());
}
