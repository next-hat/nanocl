use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use nanocl_error::io::{FromIo, IoError, IoResult};

/// Send a command to HAProxy admin socket and return the response
pub fn send_socket_command(
  socket_path: &str,
  command: &str,
) -> IoResult<String> {
  let mut stream = UnixStream::connect(socket_path).map_err(|err| {
    IoError::interrupted(
      "HAProxySocket",
      &format!("Failed to connect to socket {}: {}", socket_path, err),
    )
  })?;

  stream
    .set_read_timeout(Some(Duration::from_secs(5)))
    .map_err(|err| err.map_err_context(|| "HAProxySocket"))?;
  stream
    .set_write_timeout(Some(Duration::from_secs(5)))
    .map_err(|err| err.map_err_context(|| "HAProxySocket"))?;

  // Send command (HAProxy expects newline-terminated commands)
  let cmd = format!("{}\n", command.trim());
  stream
    .write_all(cmd.as_bytes())
    .map_err(|err| err.map_err_context(|| "HAProxySocket"))?;
  stream
    .flush()
    .map_err(|err| err.map_err_context(|| "HAProxySocket"))?;

  // Read response
  let mut response = String::new();
  stream
    .read_to_string(&mut response)
    .map_err(|err| err.map_err_context(|| "HAProxySocket"))?;

  Ok(response)
}

/// Add a backend to HAProxy at runtime
pub fn add_backend(socket_path: &str, backend_name: &str) -> IoResult<()> {
  let cmd = format!("new backend {}", backend_name);
  let response = send_socket_command(socket_path, &cmd)?;

  if response.contains("already exists") {
    // Backend already exists, not an error
    return Ok(());
  }

  if !response.is_empty() && !response.contains("New backend registered") {
    return Err(IoError::interrupted(
      "HAProxySocket",
      &format!("Failed to add backend {}: {}", backend_name, response),
    ));
  }

  Ok(())
}

/// Delete a backend from HAProxy at runtime
pub fn delete_backend(socket_path: &str, backend_name: &str) -> IoResult<()> {
  let cmd = format!("del backend {}", backend_name);
  let response = send_socket_command(socket_path, &cmd)?;

  if response.contains("No such backend") {
    // Backend doesn't exist, not an error
    return Ok(());
  }

  if !response.is_empty() && !response.contains("Backend deleted") {
    log::warn!(
      "Failed to delete backend {}: {}",
      backend_name,
      response.trim()
    );
  }

  Ok(())
}

/// Add a server to a backend at runtime
pub fn add_server(
  socket_path: &str,
  backend_name: &str,
  server_name: &str,
  address: &str,
  port: u16,
  options: &str,
) -> IoResult<()> {
  let cmd = format!(
    "add server {}/{} {}:{} {}",
    backend_name, server_name, address, port, options
  );
  let response = send_socket_command(socket_path, &cmd)?;

  if response.contains("already exists") {
    // Server already exists, not an error
    return Ok(());
  }

  if !response.is_empty() && !response.contains("New server registered") {
    return Err(IoError::interrupted(
      "HAProxySocket",
      &format!(
        "Failed to add server {}/{}: {}",
        backend_name, server_name, response
      ),
    ));
  }

  Ok(())
}

/// Delete a server from a backend at runtime
pub fn delete_server(
  socket_path: &str,
  backend_name: &str,
  server_name: &str,
) -> IoResult<()> {
  let cmd = format!("del server {}/{}", backend_name, server_name);
  let response = send_socket_command(socket_path, &cmd)?;

  if response.contains("No such server") || response.contains("No such backend")
  {
    // Server/backend doesn't exist, not an error
    return Ok(());
  }

  if !response.is_empty() && !response.contains("Server deleted") {
    log::warn!(
      "Failed to delete server {}/{}: {}",
      backend_name,
      server_name,
      response.trim()
    );
  }

  Ok(())
}

/// Set server address (update IP without restart/reload)
pub fn set_server_addr(
  socket_path: &str,
  backend_name: &str,
  server_name: &str,
  address: &str,
  port: u16,
) -> IoResult<()> {
  let cmd = format!(
    "set server {}/{} addr {} port {}",
    backend_name, server_name, address, port
  );
  let response = send_socket_command(socket_path, &cmd)?;

  if !response.is_empty() && !response.contains("changed") {
    return Err(IoError::interrupted(
      "HAProxySocket",
      &format!(
        "Failed to update server {}/{} address: {}",
        backend_name, server_name, response
      ),
    ));
  }

  Ok(())
}

/// Add an entry to HAProxy map file at runtime
pub fn add_map_entry(
  socket_path: &str,
  map_file: &str,
  key: &str,
  value: &str,
) -> IoResult<()> {
  let cmd = format!("add map {} {} {}", map_file, key, value);
  let response = send_socket_command(socket_path, &cmd)?;

  if !response.is_empty()
    && !response.contains("map entry added")
    && !response.contains("Success")
  {
    log::warn!(
      "Map add response for {} -> {}: {}",
      key,
      value,
      response.trim()
    );
  }

  Ok(())
}

/// Delete an entry from HAProxy map file at runtime
pub fn delete_map_entry(
  socket_path: &str,
  map_file: &str,
  key: &str,
) -> IoResult<()> {
  let cmd = format!("del map {} {}", map_file, key);
  let response = send_socket_command(socket_path, &cmd)?;

  if !response.is_empty()
    && !response.contains("map entry deleted")
    && !response.contains("Success")
  {
    log::warn!("Map delete response for {}: {}", key, response.trim());
  }

  Ok(())
}

/// Clear all entries from a map file at runtime
pub fn clear_map(socket_path: &str, map_file: &str) -> IoResult<()> {
  let cmd = format!("clear map {}", map_file);
  let response = send_socket_command(socket_path, &cmd)?;

  if !response.is_empty() && !response.contains("map cleared") {
    log::warn!("Map clear response: {}", response.trim());
  }

  Ok(())
}

/// Show current map entries (useful for debugging)
pub fn show_map(socket_path: &str, map_file: &str) -> IoResult<String> {
  let cmd = format!("show map {}", map_file);
  send_socket_command(socket_path, &cmd)
}

/// Sync a complete backend with all servers to HAProxy runtime
pub fn sync_backend(
  socket_path: &str,
  backend: &crate::models::BackendState,
) -> IoResult<()> {
  // Add backend first
  add_backend(socket_path, &backend.name)?;

  // Add all servers to the backend
  for server in &backend.servers {
    add_server(
      socket_path,
      &backend.name,
      &server.name,
      &server.address,
      server.port,
      &server.options,
    )?;
  }

  Ok(())
}

/// Sync multiple domain mappings to the map file
pub fn sync_domain_map(
  socket_path: &str,
  map_file: &str,
  mappings: &[crate::models::DomainMapEntry],
) -> IoResult<()> {
  // Add all mappings
  for mapping in mappings {
    add_map_entry(socket_path, map_file, &mapping.domain, &mapping.backend)?;
  }

  Ok(())
}
