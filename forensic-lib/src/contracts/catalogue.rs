use super::{Registry, RegistryError};
use std::sync::OnceLock;

/// Load the compiled-in, unavailable-only catalogue without filesystem access.
pub fn builtin_registry() -> Result<&'static Registry, RegistryError> {
    static REGISTRY: OnceLock<Result<Registry, RegistryError>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| Registry::from_json(include_str!("../../contracts/catalogue.v1.json")))
        .as_ref()
        .map_err(Clone::clone)
}
