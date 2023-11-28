use nanocl_error::io::{IoResult, FromIo};

/// Generic trait to convert a metric type into a insertable database type
pub trait ToMeticDb {
  type MetricDb;

  fn to_metric_db(self, node_name: &str) -> Self::MetricDb;
}

/// Generic trait to convert a spec type into a insertable database type and vise versa
pub trait FromSpec {
  type Spec;
  type SpecPartial;

  fn try_to_data(p: &Self::SpecPartial) -> IoResult<serde_json::Value>
  where
    Self::SpecPartial: serde::Serialize,
  {
    let mut data =
      serde_json::to_value(p).map_err(|err| err.map_err_context(|| "Spec"))?;
    if let Some(meta) = data.as_object_mut() {
      meta.remove("Metadata");
    }
    Ok(data)
  }

  fn get_data(&self) -> &serde_json::Value;

  fn to_spec(&self, p: &Self::SpecPartial) -> Self::Spec;

  fn try_from_spec_partial(
    id: &str,
    version: &str,
    p: &Self::SpecPartial,
  ) -> IoResult<Self>
  where
    Self: std::marker::Sized;

  fn try_to_spec(&self) -> IoResult<Self::Spec>
  where
    Self::SpecPartial: serde::de::DeserializeOwned,
    Self::Spec: std::marker::Sized,
  {
    let p =
      serde_json::from_value::<Self::SpecPartial>(self.get_data().clone())
        .map_err(|err| err.map_err_context(|| "Spec"))?;
    Ok(self.to_spec(&p))
  }
}

/// Trait to add relation with a spec
pub trait WithSpec {
  type Type;
  type Relation;

  fn with_spec(self, s: &Self::Relation) -> Self::Type;
}
