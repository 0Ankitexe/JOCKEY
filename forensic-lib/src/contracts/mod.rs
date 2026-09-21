//! Pure, versioned signatures. A registry never carries or invokes implementations.
mod catalogue;
mod json;
mod model;
mod validate;

pub use catalogue::builtin_registry;
pub use model::{
    FunctionContract, ImplementationAvailability, LookupError, Parameter, PossibleError,
    PossibleErrorCode, RegistryError, RegistryErrorKind,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Registry {
    functions: BTreeMap<String, FunctionContract>,
}

impl Registry {
    /// Validate the entire bounded document before creating any usable registry.
    pub fn from_json(text: &str) -> Result<Self, RegistryError> {
        let document = validate::validate(json::read(text)?)?;
        Ok(Self {
            functions: document
                .functions
                .into_iter()
                .map(|f| (f.qualified_name(), f))
                .collect(),
        })
    }
    pub fn lookup(&self, name: &str) -> Result<&FunctionContract, LookupError> {
        if !validate::valid_qualified(name) {
            return Err(LookupError::InvalidName);
        }
        self.functions.get(name).ok_or(LookupError::UnknownFunction)
    }
    pub fn functions(&self) -> impl ExactSizeIterator<Item = &FunctionContract> {
        self.functions.values()
    }
}
