#!/bin/sh
## name: release_nanocl.sh

# variables
pkg_name="nanocl"
arch=`dpkg --print-architecture`
version=`cat ./Cargo.toml | grep -m 1 "version = \"" | sed 's/[^0-9.]*\([0-9.]*\).*/\1/'`
release_path="./target/${pkg_name}_${version}_${arch}"
commit_id=`git rev-parse --verify HEAD | cut -c1-8`

export RUSTFLAGS="-C target-feature=-crt-static"

if [ -n `git diff --no-ext-diff --quiet --exit-code` ]; then
  echo "You seems to have changes please commit them before release"
  # exit 1
fi;

# clear directory
rm -fr ${release_path}
# create directories structure for package
mkdir -p ${release_path}
mkdir -p ${release_path}/DEBIAN
mkdir -p ${release_path}/usr/local/bin
mkdir -p ${release_path}/usr/local/man/man1

echo "[DOC] Generating man pages"
mkdir -p ./target/man
cargo make man > /dev/null

for file in ./target/man/*; do
  file_name=`basename ${file}`
  gzip < $file > ${release_path}/usr/local/man/man1/$file_name.gz
  pandoc --from man --to markdown < $file > ./doc/${file_name%.1}.md
done

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
echo "[BUILD] Creating release"
env OPENSSL_LIB_DIR=/usr/local/lib/ OPENSSL_INCLUDE_DIR=/usr/local/include OPENSSL_STATIC=yes cargo make release > /dev/null
cp ./target/release/${pkg_name} ${release_path}/usr/local/bin
# generate DEBIAN controll
cat > ${release_path}/DEBIAN/control <<- EOM
Package: ${pkg_name}
Version: ${version}
Architecture: ${arch}
Maintainer: next-hat team@next-hat.com
Description: A self-sufficient vms and containers manager
EOM

mkdir -p ./target/debian
dpkg-deb --build --root-owner-group ${release_path} ./target/debian/${pkg_name}_${version}_${arch}.deb
