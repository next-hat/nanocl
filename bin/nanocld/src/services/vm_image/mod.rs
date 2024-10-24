use ntex::web;

pub mod clone;
pub mod count;
pub mod create_snapshot;
pub mod delete;
pub mod import;
pub mod inspect;
pub mod list;
pub mod resize;

pub use clone::*;
pub use count::*;
pub use create_snapshot::*;
pub use delete::*;
pub use import::*;
pub use inspect::*;
pub use list::*;
pub use resize::*;

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(import_vm_image);
  config.service(list_vm_images);
  config.service(delete_vm_image);
  config.service(snapshot_vm_image);
  config.service(clone_vm_image);
  config.service(count_vm_image);
  config.service(resize_vm_image);
  config.service(inspect_vm_image);
}

#[cfg(test)]
pub mod tests {
  use futures_util::StreamExt;
  use ntex::http::StatusCode;
  use tokio_util::codec;

  use nanocl_error::io::{FromIo, IoError, IoResult};
  use nanocl_stubs::vm_image::VmImage;

  use crate::utils::tests::*;

  async fn import_image(name: &str, path: &str) -> IoResult<()> {
    let system = gen_default_test_system().await;
    let client = system.client;
    let file = tokio::fs::File::open(path).await?;
    let err_msg = format!("Unable to import image {name}:{path}");
    let stream =
      codec::FramedRead::new(file, codec::BytesCodec::new()).map(move |r| {
        let r = r?;
        let bytes = ntex::util::Bytes::from_iter(r.freeze().to_vec());
        Ok::<ntex::util::Bytes, std::io::Error>(bytes)
      });
    let mut res = client
      .post(&format!("/vms/images/{name}/import"))
      .send_stream(stream)
      .await
      .map_err(|err| err.map_err_context(|| &err_msg))?;
    let status = res.status();
    if status != StatusCode::OK {
      let error = res
        .json::<serde_json::Value>()
        .await
        .map_err(|err| err.map_err_context(|| &err_msg))?;
      println!("{:?}", error);
    }
    test_status_code!(res.status(), StatusCode::OK, &err_msg);
    Ok(())
  }

  async fn inspect_image(name: &str) -> IoResult<VmImage> {
    let system = gen_default_test_system().await;
    let client = system.client;
    let err_msg = format!("Unable to inspect image {name}");
    let mut res = client
      .get(&format!("/vms/images/{name}/inspect"))
      .send()
      .await
      .map_err(|err| err.map_err_context(|| &err_msg))?;
    if res.status() != StatusCode::OK {
      return Err(IoError::not_found("vm_image", name));
    }
    test_status_code!(res.status(), StatusCode::OK, &err_msg);
    let data = res
      .json::<VmImage>()
      .await
      .map_err(|err| err.map_err_context(|| &err_msg))?;
    Ok(data)
  }

  pub async fn ensure_test_image() {
    let name = "ubuntu-22-test";
    let path = "../../tests/ubuntu-22.04-minimal-cloudimg-amd64.img";
    if inspect_image(name).await.is_ok() {
      return;
    }
    import_image(name, path).await.unwrap();
  }

  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let name = "ubuntu-22-test-basic";
    let path = "../../tests/ubuntu-22.04-minimal-cloudimg-amd64.img";
    import_image(name, path).await.unwrap();
    let image = inspect_image(name).await.unwrap();
    assert_eq!(image.name, name);
    let mut res = client.get("/vms/images").send().await.unwrap();
    test_status_code!(res.status(), StatusCode::OK, "Unable to list images");
    let images = res.json::<Vec<VmImage>>().await.unwrap();
    assert!(images.iter().any(|i| i.name == name));
    let res = client
      .delete(&format!("/vms/images/{name}"))
      .send()
      .await
      .unwrap();
    test_status_code!(res.status(), StatusCode::OK, "Unable to delete image");
  }
}
