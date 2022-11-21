pub fn print_version() {
  const ARCH: &str = "amd64";
  const VERSION: &str = "0.1.8";
  const COMMIT_ID: &str = "dac2b536";

  println!("Arch: {}", ARCH);
  println!("Version: {}", VERSION);
  println!("Commit Id: {}", COMMIT_ID);
}
