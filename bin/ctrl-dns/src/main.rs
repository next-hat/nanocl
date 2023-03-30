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

use ntex::web;
use ntex::server::Server;
use nanocld_client::stubs::system::Event;

mod cli;
mod error;
mod utils;
mod dnsmasq;
mod service;

/// Handle events from nanocl daemon
async fn on_event(
  client: nanocld_client::NanocldClient,
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
    Event::CargoDeleted(cargo) => {
      println!("[INFO] Removing dns entries for cargo : {}", &cargo.key);
      dnsmasq.remove_domains_file(&cargo.key)?;
      utils::restart_dns_service(&client).await
    }
    // We don't care about other events
    _ => Ok(()),
  }
}

fn setup_server() -> Server {
  let server = web::HttpServer::new(|| web::App::new());

  server.run()
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
  let dnsmasq = dnsmasq::new(&conf_dir).with_dns(cli.dns);
  if let Err(err) = dnsmasq.ensure() {
    eprintln!("{err}");
    std::process::exit(1);
  }

  let server = setup_server();

  server.await?;

  Ok(())
}
