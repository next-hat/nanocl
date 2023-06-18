use std::process::Stdio;

use ntex::rt;
use ntex::web;
use ntex::util::Bytes;
use ntex::http::StatusCode;
use ntex::channel::mpsc::Receiver;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use nanocl_stubs::vm_image::{VmImageCloneStream, VmImageResizePayload};

use crate::{utils, repositories};
use nanocl_utils::http_error::HttpError;
use crate::models::{
  Pool, VmImageDbModel, QemuImgInfo, VmImageUpdateDbModel, DaemonState,
};

pub async fn delete(name: &str, pool: &Pool) -> Result<(), HttpError> {
  let vm_image = repositories::vm_image::find_by_name(name, pool).await?;

  let children = repositories::vm_image::find_by_parent(name, pool).await?;
  if !children.is_empty() {
    return Err(HttpError {
      status: StatusCode::CONFLICT,
      msg: format!(
        "Vm image {name} has children images please delete them first"
      ),
    });
  }

  let filepath = vm_image.path.clone();

  if let Err(err) = fs::remove_file(&filepath).await {
    log::warn!("Error while deleting the file {filepath}: {err}");
  }

  repositories::vm_image::delete_by_name(name, pool).await?;
  Ok(())
}

pub async fn get_info(path: &str) -> Result<QemuImgInfo, HttpError> {
  let ouput = Command::new("qemu-img")
    .args(["info", "--output=json", path])
    .output()
    .await
    .map_err(|err| HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to get info of {path}: {}", err),
    })?;

  if !ouput.status.success() {
    return Err(HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to get info of {path}: {ouput:#?}"),
    });
  }
  let info =
    serde_json::from_slice::<QemuImgInfo>(&ouput.stdout).map_err(|err| {
      HttpError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Failed to parse info of {path}: {err}"),
      }
    })?;
  Ok(info)
}

pub async fn create_snap(
  name: &str,
  size: u64,
  image: &VmImageDbModel,
  state: &DaemonState,
) -> Result<VmImageDbModel, HttpError> {
  if repositories::vm_image::find_by_name(name, &state.pool)
    .await
    .is_ok()
  {
    return Err(HttpError {
      status: StatusCode::CONFLICT,
      msg: format!("Vm image {name} already used"),
    });
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
    .map_err(|err| HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to create snapshot of {imagepath}: {}", err),
    })?;

  output.status.success().then_some(()).ok_or(HttpError {
    status: StatusCode::INTERNAL_SERVER_ERROR,
    msg: format!("Failed to create snapshot of {imagepath}: {output:#?}"),
  })?;

  let size = format!("{size}G");
  let output = Command::new("qemu-img")
    .args(["resize", &snapshotpath, &size])
    .output()
    .await
    .map_err(|err| HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to resize snapshot of {imagepath}: {}", err),
    })?;

  output.status.success().then_some(()).ok_or(HttpError {
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

  let snap_image =
    repositories::vm_image::create(&snap_image, &state.pool).await?;

  Ok(snap_image)
}

pub async fn clone(
  name: &str,
  image: &VmImageDbModel,
  state: &DaemonState,
) -> Result<Receiver<Result<Bytes, HttpError>>, HttpError> {
  if image.kind != "Snapshot" {
    return Err(HttpError {
      status: StatusCode::BAD_REQUEST,
      msg: format!("Vm image {name} is not a snapshot"),
    });
  }
  if repositories::vm_image::find_by_name(name, &state.pool)
    .await
    .is_ok()
  {
    return Err(HttpError {
      status: StatusCode::CONFLICT,
      msg: format!("Vm image {name} already used"),
    });
  }

  let (tx, rx) = ntex::channel::mpsc::channel::<Result<Bytes, HttpError>>();

  let name = name.to_owned();
  let image = image.clone();
  let daemon_conf = state.config.clone();
  let pool = state.pool.clone();
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
      .map_err(|err| HttpError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Failed to convert snapshot to base {err}"),
      }) {
      Err(err) => {
        let _ = tx.send(Err(err.clone()));
        return Err(err);
      }
      Ok(child) => child,
    };

    let mut stdout = match child.stdout.take().ok_or(HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: "Failed to convert snapshot to base".into(),
    }) {
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

    let output = match child.wait().await.map_err(|err| HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to convert snapshot to base {err}"),
    }) {
      Err(err) => {
        let _ = tx.send(Err(err.clone()));
        return Err(err);
      }
      Ok(output) => output,
    };

    if let Err(err) = output.success().then_some(()).ok_or(HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to convert snapshot to base {output:#?}"),
    }) {
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

    let new_base_image = VmImageDbModel {
      name: name.to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
      kind: "Base".into(),
      path: newbasepath.clone(),
      format: image_info.format,
      size_actual: image_info.actual_size,
      size_virtual: image_info.virtual_size,
      parent: None,
    };

    let vm = match repositories::vm_image::create(&new_base_image, &pool).await
    {
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

pub async fn resize(
  image: &VmImageDbModel,
  payload: &VmImageResizePayload,
  pool: &Pool,
) -> Result<VmImageDbModel, HttpError> {
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
      .map_err(|err| HttpError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Unable to resize image {err}"),
      })?;

  if !ouput.status.success() {
    let output = String::from_utf8(ouput.stderr).unwrap();
    return Err(HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Unable to resize image {output}"),
    });
  }

  let image_info = get_info(&imagepath).await?;
  let res = repositories::vm_image::update_by_name(
    &image.name,
    &VmImageUpdateDbModel {
      size_actual: image_info.actual_size,
      size_virtual: image_info.virtual_size,
    },
    pool,
  )
  .await?;

  Ok(res)
}

pub async fn resize_by_name(
  name: &str,
  payload: &VmImageResizePayload,
  pool: &Pool,
) -> Result<VmImageDbModel, HttpError> {
  let image = repositories::vm_image::find_by_name(name, pool).await?;
  resize(&image, payload, pool).await
}

pub async fn create(
  name: &str,
  filepath: &str,
  pool: &Pool,
) -> Result<VmImageDbModel, HttpError> {
  // Get image info
  let image_info = match utils::vm_image::get_info(filepath).await {
    Err(err) => {
      let fp2 = filepath.to_owned();
      let _ = web::block(move || std::fs::remove_file(fp2)).await;
      return Err(err);
    }
    Ok(image_info) => image_info,
  };

  let vm_image = VmImageDbModel {
    name: name.to_owned(),
    created_at: chrono::Utc::now().naive_utc(),
    kind: "Base".into(),
    format: image_info.format,
    size_actual: image_info.actual_size,
    size_virtual: image_info.virtual_size,
    path: filepath.to_owned(),
    parent: None,
  };

  let image = repositories::vm_image::create(&vm_image, pool).await?;
  Ok(image)
}
