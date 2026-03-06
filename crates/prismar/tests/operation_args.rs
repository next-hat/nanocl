use prismar::{
  CountArgs, CreateArgs, CreateManyArgs, DeleteArgs, DeleteManyArgs, FieldFilter,
  ModelFilter, NumberOps, ReadManyArgs, ReadUniqueArgs, UpdateArgs,
  UpdateManyArgs,
};

#[test]
fn operation_args_are_serde_friendly() {
  let where_filter = NumberOps::new()
    .gt(1.0)
    .into_filters()
    .into_iter()
    .fold(ModelFilter::empty(), |acc, op| {
      acc.predicate("replicas", FieldFilter::Number(op))
    });

  let args = ReadManyArgs {
    r#where: Some(where_filter.clone()),
    order_by: None,
    pagination: None,
  };

  let value = serde_json::to_value(args).unwrap();
  assert!(value.get("where").is_some());

  let read_unique = ReadUniqueArgs {
    r#where: where_filter.clone(),
  };
  let count = CountArgs {
    r#where: Some(where_filter.clone()),
  };
  let create = CreateArgs {
    data: serde_json::json!({ "name": "api" }),
  };
  let create_many = CreateManyArgs {
    data: vec![serde_json::json!({ "name": "api" })],
    skip_duplicates: Some(true),
  };
  let update = UpdateArgs {
    r#where: where_filter.clone(),
    data: serde_json::json!({ "replicas": 2 }),
  };
  let update_many = UpdateManyArgs {
    r#where: Some(where_filter.clone()),
    data: serde_json::json!({ "replicas": 3 }),
  };
  let delete = DeleteArgs {
    r#where: where_filter.clone(),
  };
  let delete_many = DeleteManyArgs {
    r#where: Some(where_filter),
  };

  for payload in [
    serde_json::to_value(read_unique).unwrap(),
    serde_json::to_value(count).unwrap(),
    serde_json::to_value(create).unwrap(),
    serde_json::to_value(create_many).unwrap(),
    serde_json::to_value(update).unwrap(),
    serde_json::to_value(update_many).unwrap(),
    serde_json::to_value(delete).unwrap(),
    serde_json::to_value(delete_many).unwrap(),
  ] {
    assert!(payload.is_object());
  }
}
