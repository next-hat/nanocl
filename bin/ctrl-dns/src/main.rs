/*
 * nanocl-ctrl-dns
 * Is the default nanocl controller for domain name is using dnsmasq.
 * It will ensure each cargo instance will own a dns entry.
 * The dns entry will be the cargo generated from the cargo key.
 * We will replace - and _ by a . and will be generated this way:
 * `nanocl.<key>.local`
 * This process should never stop by itself or by a crash.
 * It will loop till it have a connection to nanocl daemon
 * and be able to watch for his events.
 *
*/

use futures::StreamExt;
use nanocld_client::stubs::system::Event;

mod cli;
mod error;
mod utils;
mod dnsmasq;
mod service;

/// Handle events from nanocl daemon
async fn on_event(
  client: nanocld_client::NanoclClient,
  dnsmasq: dnsmasq::Dnsmasq,
  event: Event,
) -> Result<(), error::ErrorHint> {
  match &event {
    Event::CargoStarted(cargo) => {
      if cargo.name != "dns" && cargo.namespace_name != "system" {
        println!("[INFO] Generating dns entries for cargo : {}", &cargo.key);
        let domains = utils::gen_cargo_domains(cargo)?;
        dnsmasq.generate_domains_file(&cargo.key, &domains)?;
        return utils::restart_dns_service(&client).await;
      }
      Ok(())
    }
    Event::CargoDeleted(key) => {
      println!("[INFO] Removing dns entries for cargo : {}", &key);
      dnsmasq.remove_domains_file(key)?;
      utils::restart_dns_service(&client).await
    }
    // We don't care about other events
    _ => Ok(()),
  }
}

/// Main loop
/// It will loop till it have a connection to nanocl daemon
/// and be able to watch for his events.
/// It will then handle each event by calling `on_event`
/// and will try to reconnect to nanocl daemon every 2seconds if the connection is lost.
async fn run(
  client: &nanocld_client::NanoclClient,
  dnsmasq: &dnsmasq::Dnsmasq,
) {
  loop {
    println!("[INFO] Connecting to nanocl daemon...");
    match client.watch_events().await {
      Ok(mut stream) => {
        println!("[INFO] Connected to nanocl daemon");
        while let Some(event) = stream.next().await {
          // Maybe we should use a channel to send the event to the main thread
          // and also process on_event in parallel ?
          // pb: with channel we receive an Option and we need to handle this case and to regenerate the channel
          if let Err(err) =
            on_event(client.to_owned(), dnsmasq.to_owned(), event).await
          {
            eprintln!("{err}");
          }
        }
      }
      Err(err) => {
        eprintln!(
          "[WARNING] Unable to connect to nanocl daemon got error: {err}"
        );
      }
    }
    eprintln!("[WARNING] Disconnected from nanocl daemon trying to reconnect in 2 seconds");
    ntex::time::sleep(std::time::Duration::from_secs(2)).await;
  }
}

/// Main function
/// Is parsing the command line arguments,
/// ensure a minimal dnsmasq config is present,
/// it will exit with code 1 if it can't create the minimal dnsmasq config
/// then it will try to sync the cargo dns entries with the current nanocl daemon state
/// and finally it will start the main loop
#[ntex::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("nanocl-ctrl-dns v{}", env!("CARGO_PKG_VERSION"));
  let cli = cli::parse();
  let conf_dir = cli.conf_dir.to_owned().unwrap_or("/etc".into());
  let dnsmasq = dnsmasq::new(&conf_dir).with_dns(cli.dns.to_owned());
  if let Err(err) = dnsmasq.ensure() {
    eprintln!("{err}");
    std::process::exit(1);
  }
  // We don't need the cli and conf_dir anymore
  // and the variable will never drop since we loop forever
  drop(cli);
  drop(conf_dir);
  let client = nanocld_client::NanoclClient::connect_with_unix_default();
  if let Err(err) = utils::sync_daemon_state(&client, &dnsmasq).await {
    eprintln!("{err}");
  }
  run(&client, &dnsmasq).await;
  Ok(())
}
