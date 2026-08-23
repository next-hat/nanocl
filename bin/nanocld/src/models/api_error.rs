/// OpenAPI marker for the JSON body returned from an HTTP error.
pub(crate) struct ApiError;

impl utoipa::ToSchema for ApiError {
  fn name() -> std::borrow::Cow<'static, str> {
    "ApiError".into()
  }
}

impl utoipa::PartialSchema for ApiError {
  fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::SchemaType;
    use utoipa::openapi::{ObjectBuilder, Type};

    ObjectBuilder::new()
      .schema_type(SchemaType::Type(Type::Object))
      .description(Some(
        "When returning a [HttpError](nanocl_error::http::HttpError)\nthe status code is stripped and the error\nis returned as a json object with the message\nfield set to the error message.",
      ))
      .property(
        "msg",
        ObjectBuilder::new()
          .schema_type(SchemaType::Type(Type::String))
          .build(),
      )
      .required("msg")
      .build()
      .into()
  }
}
