#!/bin/sh
## name: release_nanocl.sh

# variables
pkg_name="nanocl"
arch=`dpkg --print-architecture`
version=`cat ./bin/nanocl/Cargo.toml | grep -m 1 "version = \"" | sed 's/[^0-9.]*\([0-9.]*\).*/\1/'`
release_path="./target/${pkg_name}_${version}_${arch}"

# clear directory
rm -fr ${release_path}
# create directories structure for package
mkdir -p ${release_path}
mkdir -p ${release_path}/DEBIAN
mkdir -p ${release_path}/usr/local/bin
mkdir -p ${release_path}/usr/local/man/man1

echo "[DOC] Generating man pages"
mkdir -p ./bin/nanocl/target/man
cargo make man > /dev/null

for file in ./target/man/*; do
  file_name=`basename ${file}`
  gzip < $file > ${release_path}/usr/local/man/man1/$file_name.gz
  pandoc --from man --to markdown < $file > ./doc/man/${file_name%.1}.md
done

# env OPENSSL_LIB_DIR=/usr/local/lib/ OPENSSL_INCLUDE_DIR=/usr/local/include OPENSSL_STATIC=yes cargo make release > /dev/null
cargo build --bin nanocl --release --target=x86_64-unknown-linux-musl
strip ./target/x86_64-unknown-linux-musl/release/${pkg_name}
upx ./target/x86_64-unknown-linux-musl/release/${pkg_name}
cp ./target/x86_64-unknown-linux-musl/release/${pkg_name} ${release_path}/usr/local/bin
# generate DEBIAN controll
cat > ${release_path}/DEBIAN/control <<- EOM
Package: ${pkg_name}
Version: ${version}
Architecture: ${arch}
Maintainer: next-hat team@next-hat.com
Description: A self-sufficient vms and containers orchestrator
EOM

mkdir -p ./target/debian
dpkg-deb --build --root-owner-group ${release_path} ./target/debian/${pkg_name}_${version}_${arch}.deb
