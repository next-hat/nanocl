use std::{sync::Arc, process::Stdio};

use ntex::{rt, web, util::Bytes, channel::mpsc::Receiver};
use tokio::{fs, io::AsyncReadExt, process::Command};

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::vm_image::{VmImageCloneStream, VmImageResizePayload};

use crate::{
  utils,
  repositories::generic::*,
  models::{Pool, VmImageDb, QemuImgInfo, VmImageUpdateDb, SystemState},
};

/// Delete a vm image from the database and the filesystem
pub async fn delete_by_pk(pk: &str, state: &SystemState) -> HttpResult<()> {
  let vm_image = VmImageDb::read_by_pk(pk, &state.inner.pool).await?;
  let children = VmImageDb::read_by_parent(pk, &state.inner.pool).await?;
  if !children.is_empty() {
    return Err(HttpError::conflict(format!(
      "Vm image {pk} has children images please delete them first"
    )));
  }
  let filepath = vm_image.path.clone();
  if let Err(err) = fs::remove_file(&filepath).await {
    log::warn!("Error while deleting the file {filepath}: {err}");
  }
  VmImageDb::del_by_pk(pk, &state.inner.pool).await?;
  Ok(())
}

/// Get the info of a vm image using qemu-img info command and parse the output
pub async fn get_info(path: &str) -> HttpResult<QemuImgInfo> {
  let output = Command::new("qemu-img")
    .args(["info", "--output=json", path])
    .output()
    .await
    .map_err(|err| {
      HttpError::internal_server_error(format!(
        "Failed to get info of {path}: {err}"
      ))
    })?;
  if !output.status.success() {
    return Err(HttpError::internal_server_error(format!(
      "Failed to get info of {path}: {output:#?}"
    )));
  }
  let info =
    serde_json::from_slice::<QemuImgInfo>(&output.stdout).map_err(|err| {
      HttpError::internal_server_error(format!(
        "Failed to parse info of {path}: {err}"
      ))
    })?;
  Ok(info)
}

/// Create a vm image snapshot from a `Base` vm image.
/// The snapshot is created using qemu-img create command using the `Base` image.
/// Resized to the given size it is a qcow2 image.
/// Stored in the state directory and added to the database.
/// It will be used to start a VM.
pub async fn create_snap(
  name: &str,
  size: u64,
  image: &VmImageDb,
  state: &SystemState,
) -> HttpResult<VmImageDb> {
  if VmImageDb::read_by_pk(name, &state.inner.pool).await.is_ok() {
    return Err(HttpError::conflict(format!("Vm image {name} already used")));
  }
  let img_path = image.path.clone();
  let snapshot_path =
    format!("{}/vms/images/{}.img", state.inner.config.state_dir, name);
  let output = Command::new("qemu-img")
    .args([
      "create",
      "-F",
      "qcow2",
      "-f",
      "qcow2",
      "-b",
      &img_path,
      &snapshot_path,
    ])
    .output()
    .await
    .map_err(|err| {
      HttpError::internal_server_error(format!(
        "Failed to create snapshot {name}: {err}"
      ))
    })?;
  output.status.success().then_some(()).ok_or(
    HttpError::internal_server_error(format!(
      "Failed to create snapshot {name}: {output:#?}"
    )),
  )?;
  let size = format!("{size}G");
  let output = Command::new("qemu-img")
    .args(["resize", &snapshot_path, &size])
    .output()
    .await
    .map_err(|err| {
      HttpError::internal_server_error(format!(
        "Failed to resize snapshot {img_path}: {err}"
      ))
    })?;
  output.status.success().then_some(()).ok_or(
    HttpError::internal_server_error(format!(
      "Failed to resize snapshot {name}: {output:#?}"
    )),
  )?;
  let img_info = get_info(&snapshot_path).await?;
  let snap_image = VmImageDb {
    name: name.to_owned(),
    created_at: chrono::Utc::now().naive_utc(),
    kind: "Snapshot".into(),
    path: snapshot_path.clone(),
    format: img_info.format,
    size_actual: img_info.actual_size,
    size_virtual: img_info.virtual_size,
    parent: Some(image.name.clone()),
  };
  let snap_image =
    VmImageDb::create_from(snap_image, &state.inner.pool).await?;
  Ok(snap_image)
}

/// Clone a vm image snapshot from a `Snapshot` vm image.
/// The snapshot is created using qemu-img create command using the `Snapshot` image.
/// The created clone is a qcow2 image. Stored in the state directory and added to the database.
/// It can be used as a new `Base` image.
pub async fn clone(
  name: &str,
  image: &VmImageDb,
  state: &SystemState,
) -> HttpResult<Receiver<HttpResult<Bytes>>> {
  if image.kind != "Snapshot" {
    return Err(HttpError::bad_request(format!(
      "Vm image {name} is not a snapshot"
    )));
  }
  if VmImageDb::read_by_pk(name, &state.inner.pool).await.is_ok() {
    return Err(HttpError::conflict(format!("Vm image {name} already used")));
  }
  let (tx, rx) = ntex::channel::mpsc::channel::<HttpResult<Bytes>>();
  let name = name.to_owned();
  let image = image.clone();
  let daemon_conf = state.inner.config.clone();
  let pool = Arc::clone(&state.inner.pool);
  rt::spawn(async move {
    let img_path = image.path.clone();
    let base_path =
      format!("{}/vms/images/{}.img", daemon_conf.state_dir, name);
    let mut child = match Command::new("qemu-img")
      .args(["convert", "-p", "-O", "qcow2", "-c", &img_path, &base_path])
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn()
      .map_err(|err| {
        HttpError::internal_server_error(format!(
          "Failed to convert snapshot to base {err}"
        ))
      }) {
      Err(err) => {
        let _ = tx.send(Err(err.clone()));
        return Err(err);
      }
      Ok(child) => child,
    };
    let mut stdout = match child.stdout.take().ok_or(
      HttpError::internal_server_error("Failed to convert snapshot to base"),
    ) {
      Err(err) => {
        let _ = tx.send(Err(err.clone()));
        return Err(err);
      }
      Ok(stdout) => stdout,
    };
    let tx_ptr = tx.clone();
    rt::spawn(async move {
      let mut buf = [0; 1024];
      loop {
        match stdout.read(&mut buf).await {
          Ok(n) if n > 0 => {
            let buf = String::from_utf8(buf.to_vec()).unwrap();
            let split = buf.split('/').collect::<Vec<&str>>();
            let progress = split
              .first()
              .unwrap()
              .trim()
              .trim_start_matches('(')
              .parse::<f32>()
              .unwrap();
            let stream = VmImageCloneStream::Progress(progress);
            let stream = serde_json::to_string(&stream).unwrap();
            let _ = tx_ptr.send(Ok(Bytes::from(format!("{stream}\r\n"))));
          }
          _ => break,
        }
      }
      Ok::<(), HttpError>(())
    });
    let output = match child.wait().await.map_err(|err| {
      HttpError::internal_server_error(format!(
        "Failed to convert snapshot to base {err}"
      ))
    }) {
      Err(err) => {
        let _ = tx.send(Err(err.clone()));
        return Err(err);
      }
      Ok(output) => output,
    };
    if let Err(err) =
      output
        .success()
        .then_some(())
        .ok_or(HttpError::internal_server_error(format!(
          "Failed to convert snapshot to base {output:#?}"
        )))
    {
      let _ = tx.send(Err(err.clone()));
      return Err(err);
    };
    let img_info = match get_info(&base_path).await {
      Err(err) => {
        let _ = tx.send(Err(err.clone()));
        return Err(err);
      }
      Ok(img_info) => img_info,
    };
    let new_base_image = VmImageDb {
      name: name.to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
      kind: "Base".into(),
      path: base_path.clone(),
      format: img_info.format,
      size_actual: img_info.actual_size,
      size_virtual: img_info.virtual_size,
      parent: None,
    };
    let vm = match VmImageDb::create_from(new_base_image, &pool).await {
      Err(err) => {
        let _ = tx.send(Err(err.clone().into()));
        return Err(err.into());
      }
      Ok(vm) => vm,
    };
    let stream = VmImageCloneStream::Done(vm.into());
    let stream = serde_json::to_string(&stream).unwrap();
    let _ = tx.send(Ok(Bytes::from(format!("{stream}\r\n"))));
    Ok::<(), HttpError>(())
  });
  Ok(rx)
}

/// Resize a vm image to a new size
pub async fn resize(
  image: &VmImageDb,
  payload: &VmImageResizePayload,
  pool: &Pool,
) -> HttpResult<VmImageDb> {
  let img_path = image.path.clone();
  let size = format!("{}G", payload.size);
  let mut args = vec!["resize"];
  if payload.shrink {
    args.push("--shrink");
  }
  args.push(&img_path);
  args.push(&size);
  let output =
    Command::new("qemu-img")
      .args(args)
      .output()
      .await
      .map_err(|err| {
        HttpError::internal_server_error(format!(
          "Unable to resize image {err}"
        ))
      })?;
  if !output.status.success() {
    let output = String::from_utf8(output.stdout).unwrap_or_default();
    return Err(HttpError::internal_server_error(format!(
      "Unable to resize image {output}"
    )));
  }
  let img_info = get_info(&img_path).await?;
  let res = VmImageDb::update_pk(
    &image.name,
    VmImageUpdateDb {
      size_actual: img_info.actual_size,
      size_virtual: img_info.virtual_size,
    },
    pool,
  )
  .await?;
  Ok(res)
}

/// Resize a vm image to a new size by name.
pub async fn resize_by_name(
  name: &str,
  payload: &VmImageResizePayload,
  pool: &Pool,
) -> HttpResult<VmImageDb> {
  let image = VmImageDb::read_by_pk(name, pool).await?;
  resize(&image, payload, pool).await
}

/// Create a vm image from a file as a `Base` image
pub async fn create(
  name: &str,
  filepath: &str,
  pool: &Pool,
) -> HttpResult<VmImageDb> {
  // Get image info
  let img_info = match utils::vm_image::get_info(filepath).await {
    Err(err) => {
      let fp2 = filepath.to_owned();
      let _ = web::block(move || std::fs::remove_file(fp2)).await;
      return Err(err);
    }
    Ok(img_info) => img_info,
  };
  let vm_image = VmImageDb {
    name: name.to_owned(),
    created_at: chrono::Utc::now().naive_utc(),
    kind: "Base".into(),
    format: img_info.format,
    size_actual: img_info.actual_size,
    size_virtual: img_info.virtual_size,
    path: filepath.to_owned(),
    parent: None,
  };
  let image = VmImageDb::create_from(vm_image, pool).await?;
  Ok(image)
}
