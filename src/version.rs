pub fn print_version() {
  const ARCH: &str = "amd64";
  const VERSION: &str = "0.1.1";
  const COMMIT_ID: &str = "8b787ac9";

  println!("Arch: {}\nVersion: {}\nCommit ID: {}", ARCH, VERSION, COMMIT_ID);
}
