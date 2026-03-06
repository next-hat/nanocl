#![cfg(feature = "nanocl_compat")]

use prismar::{FieldFilter, JsonFilter, ModelFilter, StringFilter};

#[test]
fn converts_supported_filters_to_generic_filter() {
  let filter = ModelFilter::empty().predicate(
    "name",
    FieldFilter::String(StringFilter::NotLike("%tmp%".into())),
  );

  let generic = nanocl_stubs::generic::GenericFilter::try_from(filter).unwrap();
  let conditions = generic.r#where.unwrap().conditions;
  assert!(conditions.contains_key("name"));
}

#[test]
fn returns_error_for_unsupported_compat_conversion() {
  let filter = ModelFilter::empty().predicate(
    "data",
    FieldFilter::Json(JsonFilter::PathLike {
      path: vec!["foo".into()],
      value: "%bar%".into(),
    }),
  );

  let result = nanocl_stubs::generic::GenericFilter::try_from(filter);
  assert!(result.is_err());
}
