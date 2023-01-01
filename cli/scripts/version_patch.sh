#!/bin/sh
## name: version_patch.sh

# variables
pkg_name="nanocl"
arch=`dpkg --print-architecture`
version=`cat ./Cargo.toml | grep -m 1 "version = \"" | sed 's/[^0-9.]*\([0-9.]*\).*/\1/'`
release_path="./target/${pkg_name}_${version}_${arch}"
commit_id=`git rev-parse --verify HEAD | cut -c1-8`

echo "[BUILD] Creating version.rs"
cat > ./src/version.rs <<- EOM
pub fn print_version() {
  const ARCH: &str = "${arch}";
  const VERSION: &str = "${version}";
  const COMMIT_ID: &str = "${commit_id}";

  println!("Arch: {}", ARCH);
  println!("Version: {}", VERSION);
  println!("Commit Id: {}", COMMIT_ID);
}
EOM
