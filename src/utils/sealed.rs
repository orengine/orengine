/// This trait can be implemented only this crate. So, it is used to prevent implementing
/// some traits in other crates.
pub(crate) trait Sealed {}
