use ntex::web;
use ntex::server::Server;
use nanocld_client::NanocldClient;
use nanocld_client::stubs::system::Event;

mod cli;
mod error;
mod utils;
mod dnsmasq;
mod service;

use dnsmasq::Dnsmasq;

/// Handle events from nanocl daemon
async fn on_event(
  client: NanocldClient,
  dnsmasq: Dnsmasq,
  event: Event,
) -> Result<(), error::ErrorHint> {
  match &event {
    Event::CargoStarted(cargo) => {
      if cargo.name == "dns" || cargo.namespace_name == "system" {
        return Ok(());
      }
      println!("[INFO] Generating dns entries for cargo : {}", &cargo.key);
      let domains = utils::gen_cargo_domains(cargo)?;
      dnsmasq.generate_domains_file(&cargo.key, &domains)?;
      utils::restart_dns_service(&client).await
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

fn setup_server(dnsmasq: &Dnsmasq) -> std::io::Result<Server> {
  let dnsmasq = dnsmasq.clone();
  let mut server = web::HttpServer::new(move || {
    web::App::new()
      .state(dnsmasq.clone())
      .configure(service::ntex_config)
  });

  server = server.bind_uds("/run/nanocl/proxy.sock")?;

  Ok(server.run())
}

fn wait_for_daemon() {
  loop {
    if std::path::Path::new("/run/nanocl/nanocl.sock").exists() {
      break;
    }

    std::thread::sleep(std::time::Duration::from_secs(5));
  }
}

#[ntex::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let cli = cli::parse();

  println!("nanocl-ncddns v{}", env!("CARGO_PKG_VERSION"));

  let conf_dir = cli.conf_dir.to_owned().unwrap_or("/etc".into());
  let dnsmasq = dnsmasq::new(&conf_dir).with_dns(cli.dns);
  if let Err(err) = dnsmasq.ensure() {
    eprintln!("{err}");
    std::process::exit(1);
  }

  let server = setup_server(&dnsmasq)?;

  server.await?;

  Ok(())
}
