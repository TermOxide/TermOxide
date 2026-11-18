#![cfg(feature = "serde")]

use serde_reflection::{ContainerFormat, Format, Tracer, TracerConfig};
use termoxide_layout::style::{BorderSides, Color, Property, Sem, Style, TextWrap, Unit};

#[test]
fn style_round_trips_through_postcard() {
    let style = Style::new()
        .fg(Color::Semantic(Sem::Accent))
        .bg(Color::Rgb(1, 2, 3))
        .bold(false)
        .width(Unit::Fr(1.5))
        .min_width(Unit::Percent(25.0))
        .border_sides(BorderSides::LEFT)
        .text_wrap(TextWrap::Word);
    let style = Style { italic: Property::Inherit, ..style };

    let bytes = postcard::to_allocvec(&style).unwrap();
    assert_eq!(postcard::from_bytes::<Style>(&bytes).unwrap(), style);
}

/// The hot-reload fingerprint traces the wire types this way; it must see
/// the payload of every property, not just `Property`'s first variant.
#[test]
fn schema_trace_sees_property_payload_types() {
    let mut tracer = Tracer::new(TracerConfig::default());
    tracer.trace_simple_type::<Style>().unwrap();
    let registry = tracer.registry_unchecked();

    assert!(registry.contains_key("Color"));
    assert!(registry.contains_key("Unit"));
    let Some(ContainerFormat::Struct(fields)) = registry.get("Style") else {
        panic!("Style is not traced as a struct");
    };
    let field = |name: &str| fields.iter().find(|f| f.name == name).map(|f| f.value.clone());
    let twice = |inner: Format| Format::Option(Box::new(Format::Option(Box::new(inner))));
    assert_eq!(field("fg"), Some(twice(Format::TypeName("Color".into()))));
    assert_eq!(field("bold"), Some(twice(Format::Bool)));
    assert_eq!(field("width"), Some(twice(Format::TypeName("Unit".into()))));
}
