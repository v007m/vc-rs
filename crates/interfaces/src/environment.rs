/// Reads the process environment for `${env.NAME}` (LLD "Vars, templates and CEL at run
/// time"). The system adapter takes one snapshot when the run starts, so every read in a run
/// sees the same values.
pub trait Environment: Send + Sync {
    /// The value of `name`, or `None` when it is not set or is not UTF-8.
    fn var(&self, name: &str) -> Option<String>;

    /// The name of every variable [`Environment::var`] reads, sorted, so a layer can list the
    /// `VIB_VAR_*` ones (D182).
    fn names(&self) -> Vec<String>;
}
