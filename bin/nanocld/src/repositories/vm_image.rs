use ntex::web;
use ntex::http::StatusCode;
use diesel::prelude::*;

async fn create(
  item: VmImageDbModel,
  pool: &Pool,
) -> Result<VmImageDbModel, HttpResponseError> {
}
