pub fn print_version() {
  const ARCH: &str = "amd64";
  const VERSION: &str = "0.1.7";
  const COMMIT_ID: &str = "d0146f87";

  println!("Arch: {}", ARCH);
  println!("Version: {}", VERSION);
  println!("Commit Id: {}", COMMIT_ID);
}
