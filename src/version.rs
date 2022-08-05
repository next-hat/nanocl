pub fn print_version() {
  const ARCH: &str = "amd64";
  const VERSION: &str = "0.1.2";
  const COMMIT_ID: &str = "8ee0bd81";

  println!("Arch: {}", ARCH);
  println!("Version: {}", VERSION);
  println!("Commit Id: {}", COMMIT_ID);
}
