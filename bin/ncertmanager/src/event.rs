use nanocl_error::http::HttpError;
use nanocld_client::NanocldClient;
use nanocld_client::stubs::system::Event;
use ntex::{rt, channel};
use futures::{StreamExt, select};

use nanocl_error::io::IoResult;
use ntex_util::channel::mpsc::{Receiver, Sender};

use crate::manager::NCertManager;
use crate::utils::event::handle_event;
use crate::utils::init::init_cert_manager;

fn get_renew_handle(
  sx: Sender<bool>,
  renew_interval: u64,
) -> ntex::rt::JoinHandle<()> {
  rt::spawn(async move {
    loop {
      ntex::time::sleep(std::time::Duration::from_secs(renew_interval)).await;
      if let Err(err) = sx.send(true) {
        log::warn!("Sx error {err}");
        sx.close();
        break;
      }
    }
  })
}

pub async fn event_loop<'a>(
  manager: &mut NCertManager<'a>,
  renew_interval: u64,
  stream: &mut Receiver<Result<Event, HttpError>>,
) {
  let (sx, mut rx) = channel::mpsc::channel::<bool>();

  let renew_handle = get_renew_handle(sx, renew_interval);

  loop {
    select! {
      event = stream.next() => {
        if handle_event(manager, event).await {
            break;
          }
      }
      option = rx.next() => {
        match option {
          Some(_) => {
            log::info!("Renew");
            manager.renew_secrets().await;
            manager.debug();
          }
          None => {
            log::error!("Renew stream end");
            break;
          }
        }
      }
      complete => {
        log::error!("Streams end");
        break
      },
      default => {
        if stream.is_closed() {
          log::error!("Event stream closed");
          renew_handle.abort();
          break;
        }
        if rx.is_closed() {
          stream.close();
          log::error!("Renew stream closed");
          break;
        }
        ntex::time::sleep(std::time::Duration::from_millis(100)).await;
      }

    }
  }
}

pub async fn init_loop(
  client: &NanocldClient,
  cert_dir: String,
  renew_interval: u64,
) -> IoResult<()> {
  loop {
    log::info!("Subscribing to nanocl daemon events..");

    match client.watch_events().await {
      Err(err) => {
        log::warn!("Unable to Subscribe to nanocl daemon events: {err}");
      }
      Ok(mut stream) => {
        log::info!("Subscribed to nanocl daemon events");
        match init_cert_manager(client, cert_dir.to_owned()).await {
          Ok(mut manager) => {
            event_loop(&mut manager, renew_interval, &mut stream).await;
          }
          Err(err) => {
            log::error!("Can't init CertManager: {err}");
          }
        }
      }
    }

    log::warn!(
      "Unsubscribed from nanocl daemon events, retrying to subscribe in 2 seconds"
    );

    ntex::time::sleep(std::time::Duration::from_secs(2)).await;
  }
}
