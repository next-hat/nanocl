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
        " failed"
      ),
      $start,
      $($other_exprs)+
    )
  };
  ($start: expr $( ,)?) => { format!("Command {} failed", $start) };
}

#[macro_export]
macro_rules! exec_cli {
  ([$($args: expr),+] $(,)?) => {{
    let args = Cli::parse_from([$($args),+]);
    execute_arg(&args).await
  }};
}

#[macro_export]
macro_rules! assert_cli_ok {
  ($cmd :expr $(, $args:expr)* $(,)?) => {
    let res = exec_cli!(
      [$cmd $(, $args)*],
    );
    assert!(res.is_ok(), "{}", format_command!($cmd $(, $args)*));
  };
}

pub use assert_cli_ok;
