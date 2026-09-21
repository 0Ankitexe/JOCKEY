//! Span-free compiler contract vocabulary. No runtime values or adapters.

macro_rules! vocabulary {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, serde::Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize)]
        pub enum $name { $(#[serde(rename = $text)] $variant),+ }
        impl $name {
            pub const ALL: [Self; [$(stringify!($variant)),+].len()] = [$(Self::$variant),+];
            #[must_use]
            pub const fn as_str(self) -> &'static str { match self { $(Self::$variant => $text),+ } }
            #[must_use]
            pub fn from_name(name: &str) -> Option<Self> { match name { $($text => Some(Self::$variant)),+, _ => None } }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.as_str()) }
        }
    };
}

mod metadata;
mod types;
pub use metadata::{Capability, Platform, Privilege};
pub use types::{NamedType, Type};
