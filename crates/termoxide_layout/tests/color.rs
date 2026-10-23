use termoxide_layout::style::color::{Color, NamedColor};

#[test]
fn every_ratatui_color_converts_back_to_itself() {
    use ratatui::style::Color as R;

    let colors = [
        R::Reset,
        R::Black,
        R::Red,
        R::Green,
        R::Yellow,
        R::Blue,
        R::Magenta,
        R::Cyan,
        R::Gray,
        R::DarkGray,
        R::LightRed,
        R::LightGreen,
        R::LightYellow,
        R::LightBlue,
        R::LightMagenta,
        R::LightCyan,
        R::White,
        R::Rgb(255, 127, 80),
        R::Indexed(240),
    ];

    for color in colors {
        assert_eq!(Color::from(color).to_ratatui(), color);
    }
}

#[test]
fn white_is_ansi_white_and_bright_white_is_ansi_bright_white() {
    assert_eq!(Color::Named(NamedColor::White).to_ratatui(), ratatui::style::Color::Gray);
    assert_eq!(Color::Named(NamedColor::BrightWhite).to_ratatui(), ratatui::style::Color::White);
}

#[test]
fn colors_round_trip_through_postcard() {
    let colors = [
        Color::Named(NamedColor::BrightCyan),
        Color::Rgb(255, 127, 80),
        Color::Indexed(240),
        Color::Inherit,
        Color::None,
    ];

    for color in colors {
        assert_eq!(
            postcard::from_bytes::<Color>(&postcard::to_allocvec(&color).unwrap()).unwrap(),
            color
        );
    }
}
