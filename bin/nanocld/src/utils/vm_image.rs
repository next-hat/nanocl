use ntex::http::StatusCode;
use tokio::fs;
use tokio::process::Command;

use nanocl_stubs::config::DaemonConfig;

use crate::repositories;
use crate::error::HttpResponseError;
use crate::models::{Pool, VmImageDbModel, QemuImgInfo};

pub async fn delete(name: &str, pool: &Pool) -> Result<(), HttpResponseError> {
  let vm_image = repositories::vm_image::find_by_name(name, pool).await?;

  let filepath = vm_image.path.clone();

  if let Err(err) = fs::remove_file(&filepath).await {
    log::warn!("Error while deleting the file {filepath}: {err}");
  }

  repositories::vm_image::delete_by_name(name, pool).await?;
  Ok(())
}

pub async fn get_info(path: &str) -> Result<QemuImgInfo, HttpResponseError> {
  let ouput = Command::new("qemu-img")
    .args(["info", "--output=json", path])
    .output()
    .await
    .map_err(|err| HttpResponseError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to get info of {path}: {}", err),
    })?;

  println!("{:?}", ouput);

  if !ouput.status.success() {
    return Err(HttpResponseError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to get info of {path}: {ouput:#?}"),
    });
  }
  let info =
    serde_json::from_slice::<QemuImgInfo>(&ouput.stdout).map_err(|err| {
      HttpResponseError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Failed to parse info of {path}: {err}"),
      }
    })?;
  Ok(info)
}

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

  output
    .status
    .success()
    .then_some(())
    .ok_or(HttpResponseError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to create snapshot of {imagepath}: {output:#?}"),
    })?;

  let output = Command::new("qemu-img")
    .args(["resize", &snapshotpath, "50G"])
    .output()
    .await
    .map_err(|err| HttpResponseError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to resize snapshot of {imagepath}: {}", err),
    })?;

  output
    .status
    .success()
    .then_some(())
    .ok_or(HttpResponseError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to resize snapshot of {imagepath}: {output:#?}"),
    })?;

  let image_info = get_info(&snapshotpath).await?;

  let snap_image = VmImageDbModel {
    name: name.to_owned(),
    created_at: chrono::Utc::now().naive_utc(),
    kind: "Snapshot".into(),
    path: snapshotpath.clone(),
    format: image_info.format,
    size_actual: image_info.actual_size,
    size_virtual: image_info.virtual_size,
    parent: Some(image.name.clone()),
  };

  let snap_image = repositories::vm_image::create(&snap_image, pool).await?;

  Ok(snap_image)
}
