use nanocl_error::io::IoResult;

/// Generic trait to convert a metric type into a insertable database type
pub trait ToMeticDb {
  type MetricDb;

  fn to_metric_db(self, node_name: &str) -> Self::MetricDb;
}

/// Generic trait to convert a spec type into a insertable database type and vise versa
pub trait FromSpec {
  type Spec;
  type SpecPartial;

  fn into_spec(self, p: &Self::SpecPartial) -> Self::Spec;

  fn try_from_spec_partial(
    id: &str,
    version: &str,
    p: &Self::SpecPartial,
  ) -> IoResult<Self>
  where
    Self: std::marker::Sized;

  fn try_to_spec(self) -> IoResult<Self::Spec>;
}
