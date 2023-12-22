/// Trait to add relation with a spec
pub trait WithSpec {
  type Output;
  type Relation;

  fn with_spec(self, s: &Self::Relation) -> Self::Output;
}
