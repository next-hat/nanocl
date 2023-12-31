use std::{sync::Arc, process::Stdio};

use ntex::{rt, web, util::Bytes, channel::mpsc::Receiver};
use tokio::{fs, io::AsyncReadExt, process::Command};

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::vm_image::{VmImageCloneStream, VmImageResizePayload};

use crate::{
  utils,
  repositories::generic::*,
  models::{Pool, VmImageDb, QemuImgInfo, VmImageUpdateDb, DaemonState},
};

/// Delete a vm image from the database and from the filesystem
pub(crate) async fn delete_by_name(name: &str, pool: &Pool) -> HttpResult<()> {
  let vm_image = VmImageDb::read_by_pk(name, pool).await?;
  let children = VmImageDb::read_by_parent(name, pool).await?;
  if !children.is_empty() {
    return Err(HttpError::conflict(format!(
      "Vm image {name} has children images please delete them first"
    )));
  }
  let filepath = vm_image.path.clone();
  if let Err(err) = fs::remove_file(&filepath).await {
    log::warn!("Error while deleting the file {filepath}: {err}");
  }
  VmImageDb::del_by_pk(name, pool).await?;
  Ok(())
}

/// Get the info of a vm image using qemu-img info command and parse the output
pub(crate) async fn get_info(path: &str) -> HttpResult<QemuImgInfo> {
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
pub(crate) async fn create_snap(
  name: &str,
  size: u64,
  image: &VmImageDb,
  state: &DaemonState,
) -> HttpResult<VmImageDb> {
  if VmImageDb::read_by_pk(name, &state.pool).await.is_ok() {
    return Err(HttpError::conflict(format!("Vm image {name} already used")));
  }
  let imagepath = image.path.clone();
  let snapshotpath =
    format!("{}/vms/images/{}.img", state.config.state_dir, name);
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
    .args(["resize", &snapshotpath, &size])
    .output()
    .await
    .map_err(|err| {
      HttpError::internal_server_error(format!(
        "Failed to resize snapshot {imagepath}: {err}"
      ))
    })?;
  output.status.success().then_some(()).ok_or(
    HttpError::internal_server_error(format!(
      "Failed to resize snapshot {name}: {output:#?}"
    )),
  )?;
  let image_info = get_info(&snapshotpath).await?;
  let snap_image = VmImageDb {
    name: name.to_owned(),
    created_at: chrono::Utc::now().naive_utc(),
    kind: "Snapshot".into(),
    path: snapshotpath.clone(),
    format: image_info.format,
    size_actual: image_info.actual_size,
    size_virtual: image_info.virtual_size,
    parent: Some(image.name.clone()),
  };
  let snap_image = VmImageDb::create_from(snap_image, &state.pool).await?;
  Ok(snap_image)
}

/// Clone a vm image snapshot from a `Snapshot` vm image.
/// The snapshot is created using qemu-img create command using the `Snapshot` image.
/// The created clone is a qcow2 image. Stored in the state directory and added to the database.
/// It can be used as a new `Base` image.
pub(crate) async fn clone(
  name: &str,
  image: &VmImageDb,
  state: &DaemonState,
) -> HttpResult<Receiver<HttpResult<Bytes>>> {
  if image.kind != "Snapshot" {
    return Err(HttpError::bad_request(format!(
      "Vm image {name} is not a snapshot"
    )));
  }
  if VmImageDb::read_by_pk(name, &state.pool).await.is_ok() {
    return Err(HttpError::conflict(format!("Vm image {name} already used")));
  }
  let (tx, rx) = ntex::channel::mpsc::channel::<HttpResult<Bytes>>();
  let name = name.to_owned();
  let image = image.clone();
  let daemon_conf = state.config.clone();
  let pool = Arc::clone(&state.pool);
  rt::spawn(async move {
    let imagepath = image.path.clone();
    let newbasepath =
      format!("{}/vms/images/{}.img", daemon_conf.state_dir, name);
    let mut child = match Command::new("qemu-img")
      .args([
        "convert",
        "-p",
        "-O",
        "qcow2",
        "-c",
        &imagepath,
        &newbasepath,
      ])
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
    let txpg = tx.clone();
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
            let _ = txpg.send(Ok(Bytes::from(format!("{stream}\r\n"))));
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
    let image_info = match get_info(&newbasepath).await {
      Err(err) => {
        let _ = tx.send(Err(err.clone()));
        return Err(err);
      }
      Ok(image_info) => image_info,
    };
    let new_base_image = VmImageDb {
      name: name.to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
      kind: "Base".into(),
      path: newbasepath.clone(),
      format: image_info.format,
      size_actual: image_info.actual_size,
      size_virtual: image_info.virtual_size,
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
pub(crate) async fn resize(
  image: &VmImageDb,
  payload: &VmImageResizePayload,
  pool: &Pool,
) -> HttpResult<VmImageDb> {
  let imagepath = image.path.clone();
  let size = format!("{}G", payload.size);
  let mut args = vec!["resize"];
  if payload.shrink {
    args.push("--shrink");
  }
  args.push(&imagepath);
  args.push(&size);
  let ouput =
    Command::new("qemu-img")
      .args(args)
      .output()
      .await
      .map_err(|err| {
        HttpError::internal_server_error(format!(
          "Unable to resize image {err}"
        ))
      })?;
  if !ouput.status.success() {
    let output = String::from_utf8(ouput.stdout).unwrap_or_default();
    return Err(HttpError::internal_server_error(format!(
      "Unable to resize image {output}"
    )));
  }
  let image_info = get_info(&imagepath).await?;
  let res = VmImageDb::update_pk(
    &image.name,
    VmImageUpdateDb {
      size_actual: image_info.actual_size,
      size_virtual: image_info.virtual_size,
    },
    pool,
  )
  .await?;
  Ok(res)
}

/// Resize a vm image to a new size by name.
pub(crate) async fn resize_by_name(
  name: &str,
  payload: &VmImageResizePayload,
  pool: &Pool,
) -> HttpResult<VmImageDb> {
  let image = VmImageDb::read_by_pk(name, pool).await?;
  resize(&image, payload, pool).await
}

/// Create a vm image from a file as a `Base` image
pub(crate) async fn create(
  name: &str,
  filepath: &str,
  pool: &Pool,
) -> HttpResult<VmImageDb> {
  // Get image info
  let image_info = match utils::vm_image::get_info(filepath).await {
    Err(err) => {
      let fp2 = filepath.to_owned();
      let _ = web::block(move || std::fs::remove_file(fp2)).await;
      return Err(err);
    }
    Ok(image_info) => image_info,
  };
  let vm_image = VmImageDb {
    name: name.to_owned(),
    created_at: chrono::Utc::now().naive_utc(),
    kind: "Base".into(),
    format: image_info.format,
    size_actual: image_info.actual_size,
    size_virtual: image_info.virtual_size,
    path: filepath.to_owned(),
    parent: None,
  };
  let image = VmImageDb::create_from(vm_image, pool).await?;
  Ok(image)
}
