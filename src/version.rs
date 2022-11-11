pub fn print_version() {
  const ARCH: &str = "amd64";
  const VERSION: &str = "0.1.5";
  const COMMIT_ID: &str = "a42e4361";

  println!("Arch: {}", ARCH);
  println!("Version: {}", VERSION);
  println!("Commit Id: {}", COMMIT_ID);
}
