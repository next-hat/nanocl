use ntex::http::StatusCode;
use tokio::fs;
use tokio::process::Command;

use nanocl_stubs::config::DaemonConfig;

use crate::repositories;
use crate::error::HttpResponseError;
use crate::models::{Pool, VmImageDbModel};

pub async fn create_snap(
  name: &str,
  image: &VmImageDbModel,
  daemon_conf: &DaemonConfig,
  pool: &Pool,
) -> Result<VmImageDbModel, HttpResponseError> {
  let imagepath = image.path.clone();
  let snapshotpath =
    format!("{}/vms/images/{}.img", daemon_conf.state_dir, name);

  let output = Command::new("qemu-img")
    .args([
      "create",
      "-F",
      "qcow2",
      "-f",
      "qcow2",
      "-b",
      &imagepath,
      &snapshotpath,
    ])
    .output()
    .await
    .map_err(|err| HttpResponseError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to create snapshot of {imagepath}: {}", err),
    })?;

  let output = Command::new("qemu-img")
    .args(["resize", &snapshotpath, "50G"])
    .output()
    .await
    .map_err(|err| HttpResponseError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to resize snapshot of {imagepath}: {}", err),
    })?;

  println!("qemu-img output: {output:#?}");

  let metadata =
    fs::metadata(&snapshotpath)
      .await
      .map_err(|err| HttpResponseError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Failed to get metadata of {snapshotpath}: {}", err),
      })?;

  let snap_image = VmImageDbModel {
    name: name.to_owned(),
    created_at: chrono::Utc::now().naive_utc(),
    kind: "Snapshot".into(),
    path: snapshotpath.clone(),
    size: metadata.len() as i64,
    parent: Some(image.name.clone()),
  };

  let snap_image = repositories::vm_image::create(snap_image, pool).await?;

  Ok(snap_image)
}
