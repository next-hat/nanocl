pub mod state;
pub mod print;
pub mod math;
pub mod docker;
pub mod installer;
pub mod dialog;
pub mod context;
pub mod hash;

#[cfg(test)]
pub mod tests {

  pub fn get_test_client() -> NanocldClient {
    NanocldClient::connect_to("http://nanocl.internal:8585", None)
  }

  #[macro_export]
  macro_rules! exprs_as_bracket {
    () => {};
    ($expr: expr  $(,)?) => {
      " {}"
    };
    ($expr: expr, $($other_exprs: tt),+) => {
      concat!(" {}", exprs_as_bracket!($($other_exprs),+))
    };
  }

  #[macro_export]
  macro_rules! format_command {
    ($start: expr, $($other_exprs:tt)+) => {
      format!(
        concat!(
          "Command {}",
          exprs_as_bracket!($($other_exprs)+),
          " doesn't give expected result"
        ),
        $start,
        $($other_exprs)+
      )
    };
    ($start: expr $( ,)?) => { format!("Command {} doesn't give expected result", $start) };
  }

  #[macro_export]
  macro_rules! exec_cli {
    ([$($args: expr),+] $(,)?) => {{
      let args = Cli::try_parse_from(["nanocl", $($args),+]).expect("Can't parse command");
      execute_arg(&args).await
    }};
  }

  #[macro_export]
  macro_rules! assert_cli_ok {
    ($cmd :expr $(, $args:expr)* $(,)?) => {
      let res = exec_cli!(
        [$cmd $(, $args)*],
      );
      assert!(res.is_ok(), "{:#?} {}", res, format_command!($cmd $(, $args)*));
    };
  }

  #[macro_export]
  macro_rules! assert_cli_err {
    ($cmd :expr $(, $args:expr)* $(,)?) => {
      let res = exec_cli!(
        [$cmd $(, $args)*],
      );
      assert!(res.is_err(), "{:#?} {}", res, format_command!($cmd $(, $args)*));
    };
  }

  #[macro_export]
  macro_rules! assert_cargo_state {
    ($client :expr, $cargo_name:expr, $namespace_option:expr, $state_str:expr) => {
      let res = $client
        .inspect_cargo($cargo_name, $namespace_option)
        .await
        .expect(&format!(
          "Cargo {} in namespace {:#?} doesn't exists",
          $cargo_name, $namespace_option
        ));
      assert_eq!(
        res
          .instances
          .get(0)
          .expect(&format!(
            "No container {} in namespace {:#?} instance found",
            $cargo_name, $namespace_option
          ))
          .container
          .state
          .clone()
          .unwrap_or_default()
          .status
          .unwrap_or(bollard_next::models::ContainerStateStatusEnum::EMPTY)
          .to_string(),
        $state_str.to_owned()
      );
    };
  }

  #[macro_export]
  macro_rules! assert_cargo_exists {
    ($client :expr, $cargo_name:expr, $namespace_option:expr) => {
      let res = $client.inspect_cargo($cargo_name, $namespace_option).await;
      assert!(
        res.is_ok(),
        "Cargo {} in namespace {:#?} doesn't exists : {:#?}",
        $cargo_name,
        $namespace_option,
        res
      );
    };
  }

  #[macro_export]
  macro_rules! assert_cargo_not_exists {
    ($client :expr, $cargo_name:expr, $namespace_option:expr) => {
      let res = $client.inspect_cargo($cargo_name, $namespace_option).await;
      assert!(
        res.is_err(),
        "Cargo {} in namespace {:#?} exists : {:#?}",
        $cargo_name,
        $namespace_option,
        res
      );
    };
  }

  pub use assert_cli_ok;
  use nanocld_client::NanocldClient;
}
