pub fn print_version() {
  const ARCH: &str = "amd64";
  const VERSION: &str = "0.1.6";
  const COMMIT_ID: &str = "a5ff34de";

  println!("Arch: {}", ARCH);
  println!("Version: {}", VERSION);
  println!("Commit Id: {}", COMMIT_ID);
}
