/// Existing objects that can be suggested for a command argument.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionKind {
  Cargo,
  Vm,
  Job,
  Resource,
  Namespace,
  Secret,
  Context,
  Process,
  CargoContainer,
  CargoHistory,
  ResourceHistory,
  Event,
  Metric,
}
