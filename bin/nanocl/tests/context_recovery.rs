//! These subprocess/filesystem regressions are compiled locally and run in CI.

use std::{fs, net::TcpListener, path::PathBuf, process::Command};

struct ContextFixture {
  home: PathBuf,
  daemon: TcpListener,
}

impl ContextFixture {
  fn new() -> Self {
    let home = std::env::temp_dir()
      .join(format!("nanocl-context-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(home.join(".nanocl/contexts")).unwrap();
    let daemon = TcpListener::bind("127.0.0.1:0").unwrap();
    daemon.set_nonblocking(true).unwrap();
    Self { home, daemon }
  }

  fn cli(&self, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_nanocl"))
      .args(args)
      .current_dir(&self.home)
      .env("HOME", &self.home)
      // Explicit HOST reaches the fixture in both normal and dev/test builds.
      // Even this override must not silently conceal an invalid selection.
      .env(
        "HOST",
        format!("http://{}", self.daemon.local_addr().unwrap()),
      )
      .env_remove("CERT")
      .env_remove("CERT_KEY")
      .env_remove("CERT_CA")
      .env_remove("COMPLETE")
      .env_remove("NANOCL_HOST")
      .output()
      .unwrap()
  }

  fn assert_no_requests(&self) {
    assert!(
      matches!(self.daemon.accept(), Err(err) if err.kind() == std::io::ErrorKind::WouldBlock)
    );
  }
}

impl Drop for ContextFixture {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.home);
  }
}

#[test]
fn unavailable_selected_context_fails_without_contact_or_selection_change() {
  for context in [
    None,
    Some("Name: ["),
    Some("Name: broken\nMetaData: { Description: broken }\nEndpoints: {}\n"),
  ] {
    let fixture = ContextFixture::new();
    let config_path = fixture.home.join(".nanocl/conf.yml");
    let original = "CurrentContext: broken\nDisplayFormat: Json\n";
    fs::write(&config_path, original).unwrap();
    if let Some(context) = context {
      fs::write(fixture.home.join(".nanocl/contexts/broken.yml"), context)
        .unwrap();
    }
    let result = fixture.cli(&["namespace", "create", "must-not-be-created"]);
    assert!(!result.status.success());
    let diagnostic = String::from_utf8_lossy(&result.stderr);
    assert!(
      diagnostic.contains("context") || diagnostic.contains("Context"),
      "{diagnostic}"
    );
    assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
    fixture.assert_no_requests();
    assert!(fixture.cli(&["context", "use", "default"]).status.success());
    let config: serde_yaml::Value =
      serde_yaml::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(config["CurrentContext"].as_str(), Some("default"));
    assert_eq!(config["DisplayFormat"].as_str(), Some("Json"));
    fixture.assert_no_requests();
  }
}

#[test]
fn context_import_and_switch_can_repair_a_broken_selected_connection() {
  let fixture = ContextFixture::new();
  fs::write(
    fixture.home.join(".nanocl/conf.yml"),
    "CurrentContext: broken\n",
  )
  .unwrap();
  let source = fixture.home.join("replacement.yml");
  fs::write(&source, "Name: replacement\nMetaData: { Description: repaired }\nEndpoints:\n  Nanocl:\n    Host: http://127.0.0.1:1\n").unwrap();
  assert!(
    fixture
      .cli(&["context", "from", source.to_str().unwrap()])
      .status
      .success()
  );
  assert!(
    fixture
      .cli(&["context", "use", "replacement"])
      .status
      .success()
  );
  assert!(fixture.cli(&["context", "ls"]).status.success());
  fixture.assert_no_requests();
}

#[test]
fn malformed_user_config_fails_until_explicit_local_switch() {
  let fixture = ContextFixture::new();
  let config_path = fixture.home.join(".nanocl/conf.yml");
  fs::write(&config_path, "CurrentContext: [").unwrap();
  assert!(
    !fixture
      .cli(&["namespace", "create", "must-not-be-created"])
      .status
      .success()
  );
  assert_eq!(
    fs::read_to_string(&config_path).unwrap(),
    "CurrentContext: ["
  );
  assert!(fixture.cli(&["context", "use", "default"]).status.success());
  fixture.assert_no_requests();
}
