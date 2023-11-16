/// ## Exec cargo wait
///
/// Execute the `nanocl cargo wait` command towait the end of a cargo
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli configuration
/// * [args](CargoArg) Cargo arguments
/// * [opts](CargoWaitOpts) Cargo command
///
/// ## Return
///
/// * [Result](Result) Result of the operation
///   * [Ok](()) Operation was successful
///   * [Err](nanocl_error::io::IoError) Operation failed
///
// async fn exec_cargo_wait(
//   cli_conf: &CliConfig,
//   args: &CargoArg,
//   opts: &CargoWaitOpts,
// ) -> IoResult<()> {
//   let client = &cli_conf.client;
//   let query = CargoWaitQuery {
//     condition: opts.condition.to_owned(),
//     namespace: args.namespace.to_owned(),
//   };
//   let mut stream = client.wait_cargo(&opts.name, Some(&query)).await?;
//   let mut has_error: bool = false;
//   while let Some(stream) = stream.next().await {
//     match stream {
//       Ok(wait_response) => match wait_response.status_code {
//         0 => {
//           eprintln!(
//             "Container {} {} ended successfully",
//             opts.name, wait_response.container_id
//           );
//         }
//         code => {
//           eprintln!(
//             "Container {} {} returned error code : {code}",
//             opts.name, wait_response.container_id
//           );
//           has_error = true;
//         }
//       },
//       Err(err) => {
//         eprintln!("Error: {err}");
//         break;
//       }
//     }
//   }
//   if has_error {
//     process::exit(1);
//   }
//   Ok(())
// }
