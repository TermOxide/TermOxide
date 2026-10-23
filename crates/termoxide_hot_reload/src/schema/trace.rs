//! Traces a type's shape by deserializing it from nothing, again and again:
//! each pass takes every enum it meets one variant further, until all of them
//! have been through every variant.

use std::{
    any::type_name,
    collections::{BTreeMap, BTreeSet},
    fmt,
    mem,
};

use serde::de::{
    self,
    DeserializeOwned,
    DeserializeSeed,
    EnumAccess,
    IntoDeserializer,
    MapAccess,
    SeqAccess,
    VariantAccess,
    Visitor,
    value::U32Deserializer,
};

/// Far above what any type needs: a pass that leaves an enum incomplete takes
/// at least one variant further.
const MAX_PASSES: usize = 100_000;
/// How deep types may nest before the trace gives up on a type that recurses
/// without an end.
const MAX_DEPTH: usize = 256;

/// `T`'s shape as text: its own, then every struct and enum it contains, by
/// their `serde` names, with every field and every variant in order.
pub(super) fn shape<T: DeserializeOwned>() -> Result<String, Error> {
    let mut tracer = Tracer::default();
    let mut root = String::new();
    for _ in 0..MAX_PASSES {
        tracer.incomplete = false;
        root.clear();
        T::deserialize(Shape { tracer: &mut tracer, out: &mut root })?;
        if !tracer.incomplete {
            let enums: BTreeSet<_> = tracer
                .enums
                .into_values()
                .map(|traced| {
                    let variants: Vec<_> = traced.variants.into_iter().map(|v| (v.name, v.shape)).collect();
                    (traced.name, variants)
                })
                .collect();
            return Ok(format!("{root}\n{:?}\n{enums:?}", tracer.structs));
        }
    }
    Err(Error(String::from("its enums never finish tracing")))
}

#[derive(Debug)]
pub(super) struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
}

impl std::error::Error for Error {}

impl de::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Self { Self(message.to_string()) }
}

#[derive(Default)]
struct Tracer {
    structs: BTreeSet<(&'static str, String)>,
    /// By Rust type name rather than `serde` name, which two instances of a
    /// generic enum share.
    enums: BTreeMap<&'static str, Enum>,
    /// The Rust type names of the structs and enums being traced, outermost
    /// first.
    open: Vec<&'static str>,
    /// How many of `open` are inside themselves. Inside one, nothing is
    /// recorded and sequences and options are empty, so recursion ends.
    reentered: usize,
    /// An enum met in this pass still has variants to trace.
    incomplete: bool,
}

struct Enum {
    name: &'static str,
    variants: Vec<Variant>,
}

struct Variant {
    name: &'static str,
    shape: Option<String>,
    /// Traced, along with every enum inside it.
    done: bool,
}

impl Tracer {
    /// Opens the type `key`. Returns how many times it is open already.
    fn enter(&mut self, key: &'static str) -> Result<usize, Error> {
        if self.open.len() >= MAX_DEPTH {
            return Err(Error(format!("{key} nests without an end")));
        }
        let seen = self.open.iter().filter(|open| **open == key).count();
        self.open.push(key);
        if seen > 0 {
            self.reentered += 1;
        }
        Ok(seen)
    }

    fn leave(&mut self, seen: usize) {
        self.open.pop();
        if seen > 0 {
            self.reentered -= 1;
        }
    }

    fn record_struct(&mut self, seen: usize, name: &'static str, shape: String) {
        if seen == 0 && self.reentered == 0 {
            self.structs.insert((name, shape));
        }
    }
}

/// Deserializes a value of any type, writing the type's shape to `out`.
struct Shape<'a> {
    tracer: &'a mut Tracer,
    out: &'a mut String,
}

macro_rules! primitives {
    ($($method:ident => $visit:ident($($value:expr)?), $name:literal;)*) => {
        $(
            fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
                self.out.push_str($name);
                visitor.$visit($($value)?)
            }
        )*
    };
}

impl<'de> de::Deserializer<'de> for Shape<'_> {
    type Error = Error;

    // 1 rather than 0, which the non-zero integers refuse.
    primitives! {
        deserialize_bool => visit_bool(false), "bool";
        deserialize_i8 => visit_i8(1), "i8";
        deserialize_i16 => visit_i16(1), "i16";
        deserialize_i32 => visit_i32(1), "i32";
        deserialize_i64 => visit_i64(1), "i64";
        deserialize_i128 => visit_i128(1), "i128";
        deserialize_u8 => visit_u8(1), "u8";
        deserialize_u16 => visit_u16(1), "u16";
        deserialize_u32 => visit_u32(1), "u32";
        deserialize_u64 => visit_u64(1), "u64";
        deserialize_u128 => visit_u128(1), "u128";
        deserialize_f32 => visit_f32(1.0), "f32";
        deserialize_f64 => visit_f64(1.0), "f64";
        deserialize_char => visit_char('a'), "char";
        deserialize_str => visit_borrowed_str(""), "str";
        deserialize_string => visit_borrowed_str(""), "str";
        deserialize_bytes => visit_borrowed_bytes(&[]), "bytes";
        deserialize_byte_buf => visit_borrowed_bytes(&[]), "bytes";
        deserialize_unit => visit_unit(), "unit";
    }

    fn is_human_readable(&self) -> bool { false }

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Error> {
        Err(Error(String::from("a type that describes itself has no fixed shape")))
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_any(visitor)
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_any(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.out.push_str("option<");
        let value = if self.tracer.reentered > 0 {
            visitor.visit_none()
        } else {
            visitor.visit_some(Shape { tracer: self.tracer, out: self.out })
        }?;
        self.out.push('>');
        Ok(value)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.out.push_str("seq<");
        let left = usize::from(self.tracer.reentered == 0);
        let value = visitor.visit_seq(Fields { tracer: self.tracer, out: self.out, names: None, at: 0, left })?;
        self.out.push('>');
        Ok(value)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.out.push_str("map<");
        let left = self.tracer.reentered == 0;
        let value = visitor.visit_map(Entry { tracer: self.tracer, out: self.out, left })?;
        self.out.push('>');
        Ok(value)
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        self.out.push('(');
        let value = visitor.visit_seq(Fields { tracer: self.tracer, out: self.out, names: None, at: 0, left: len })?;
        self.out.push(')');
        Ok(value)
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(self, name: &'static str, visitor: V) -> Result<V::Value, Error> {
        self.out.push_str(name);
        if self.tracer.reentered == 0 {
            self.tracer.structs.insert((name, String::new()));
        }
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, name: &'static str, visitor: V) -> Result<V::Value, Error> {
        self.out.push_str(name);
        let seen = self.tracer.enter(type_name::<V::Value>())?;
        let mut shape = String::from("(");
        let value = visitor.visit_newtype_struct(Shape { tracer: self.tracer, out: &mut shape })?;
        shape.push(')');
        self.tracer.leave(seen);
        self.tracer.record_struct(seen, name, shape);
        Ok(value)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.out.push_str(name);
        let seen = self.tracer.enter(type_name::<V::Value>())?;
        let mut shape = String::from("(");
        let fields = Fields { tracer: self.tracer, out: &mut shape, names: None, at: 0, left: len };
        let value = visitor.visit_seq(fields)?;
        shape.push(')');
        self.tracer.leave(seen);
        self.tracer.record_struct(seen, name, shape);
        Ok(value)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.out.push_str(name);
        let seen = self.tracer.enter(type_name::<V::Value>())?;
        let mut shape = String::from("{");
        let traced = Fields {
            tracer: self.tracer,
            out: &mut shape,
            names: Some(fields),
            at: 0,
            left: fields.len(),
        };
        let value = visitor.visit_seq(traced)?;
        shape.push('}');
        self.tracer.leave(seen);
        self.tracer.record_struct(seen, name, shape);
        Ok(value)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        if variants.is_empty() {
            return Err(Error(format!("{name} has no variant to build")));
        }
        self.out.push_str(name);
        let key = type_name::<V::Value>();
        let tracer = self.tracer;
        let seen = tracer.enter(key)?;
        let next = tracer
            .enums
            .entry(key)
            .or_insert_with(|| {
                let variants = variants
                    .iter()
                    .map(|&name| Variant { name, shape: None, done: false })
                    .collect();
                Enum { name, variants }
            })
            .variants
            .iter()
            .position(|variant| !variant.done);
        // Inside itself, an enum takes its variants in turn, so one that ends
        // the recursion comes up.
        let recorded = seen == 0 && tracer.reentered == 0;
        let index = match next {
            Some(next) if recorded => next,
            _ => seen % variants.len(),
        };

        let outside = mem::replace(&mut tracer.incomplete, false);
        let mut shape = String::new();
        let variant = TracedVariant { tracer: &mut *tracer, out: &mut shape, index };
        let value = visitor.visit_enum(variant)?;
        tracer.leave(seen);

        let inside = tracer.incomplete;
        let mut remaining = false;
        if recorded && let Some(traced) = tracer.enums.get_mut(key) {
            if next.is_some()
                && let Some(variant) = traced.variants.get_mut(index)
            {
                variant.shape = Some(shape);
                variant.done = !inside;
            }
            remaining = traced.variants.iter().any(|variant| !variant.done);
        }
        tracer.incomplete = outside || inside || remaining;
        Ok(value)
    }
}

/// The fields of a struct, a tuple or a variant, or the one element a
/// sequence is traced with.
struct Fields<'a> {
    tracer: &'a mut Tracer,
    out: &'a mut String,
    names: Option<&'static [&'static str]>,
    at: usize,
    left: usize,
}

impl<'de> SeqAccess<'de> for Fields<'_> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>, Error> {
        if self.left == 0 {
            return Ok(None);
        }
        if self.at > 0 {
            self.out.push(',');
        }
        if let Some(name) = self.names.and_then(|names| names.get(self.at)) {
            self.out.push_str(name);
            self.out.push(':');
        }
        self.at += 1;
        self.left -= 1;
        seed.deserialize(Shape { tracer: self.tracer, out: self.out }).map(Some)
    }
}

/// The one entry a map is traced with.
struct Entry<'a> {
    tracer: &'a mut Tracer,
    out: &'a mut String,
    left: bool,
}

impl<'de> MapAccess<'de> for Entry<'_> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, Error> {
        if !mem::take(&mut self.left) {
            return Ok(None);
        }
        seed.deserialize(Shape { tracer: self.tracer, out: self.out }).map(Some)
    }

    fn next_value_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Value, Error> {
        self.out.push(',');
        seed.deserialize(Shape { tracer: self.tracer, out: self.out })
    }
}

/// The variant at `index` of an enum.
struct TracedVariant<'a> {
    tracer: &'a mut Tracer,
    out: &'a mut String,
    index: usize,
}

impl<'de> EnumAccess<'de> for TracedVariant<'_> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<(T::Value, Self), Error> {
        let index = u32::try_from(self.index).map_err(|_| Error(String::from("an enum has too many variants")))?;
        let index: U32Deserializer<Error> = index.into_deserializer();
        Ok((seed.deserialize(index)?, self))
    }
}

impl<'de> VariantAccess<'de> for TracedVariant<'_> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> { Ok(()) }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        self.out.push('(');
        let value = seed.deserialize(Shape { tracer: self.tracer, out: self.out })?;
        self.out.push(')');
        Ok(value)
    }

    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_tuple(Shape { tracer: self.tracer, out: self.out }, len, visitor)
    }

    fn struct_variant<V: Visitor<'de>>(self, fields: &'static [&'static str], visitor: V) -> Result<V::Value, Error> {
        self.out.push('{');
        let traced = Fields {
            tracer: self.tracer,
            out: self.out,
            names: Some(fields),
            at: 0,
            left: fields.len(),
        };
        let value = visitor.visit_seq(traced)?;
        self.out.push('}');
        Ok(value)
    }
}
