use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::generic::{
  GenericFilter, GenericFilterNsp, GenericListQuery, GenericListQueryNsp,
};

pub fn parse_qs_filter(qs: &GenericListQuery) -> HttpResult<GenericFilter> {
  GenericFilter::try_from(qs.clone()).map_err(HttpError::bad_request)
}

pub fn parse_qs_nsp_filter(
  qs: &GenericListQueryNsp,
) -> HttpResult<GenericFilterNsp> {
  GenericFilterNsp::try_from(qs.clone()).map_err(HttpError::bad_request)
}
