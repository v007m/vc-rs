/// Errors raised while building model data.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    /// The string table already holds as many strings as a [`crate::Sym`] can address.
    #[error("the string table is full ({0} strings)")]
    StringTableFull(usize),
}
