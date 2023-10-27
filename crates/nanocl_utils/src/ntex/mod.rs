pub mod middlewares;

#[cfg(feature = "ntex_swagger")]
pub mod swagger;

#[cfg(feature = "ntex_test_client")]
pub mod test_client;
