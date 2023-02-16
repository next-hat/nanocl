#!/bin/sh
## name: release_nanocld.sh
set -e -x

# variables
pkg_name="nanocld"
arch=`dpkg --print-architecture`
version=`cat ./bin/nanocld/Cargo.toml | grep -m 1 "version = \"" | sed 's/[^0-9.]*\([0-9.]*\).*/\1/'`
release_path="./target/${pkg_name}_${version}_${arch}"

export RUSTFLAGS="-C target-feature=-crt-static"

# clean directory
rm -fr ${release_path}
# create directories structure for package
mkdir -p ${release_path}
mkdir -p ${release_path}/DEBIAN
mkdir -p ${release_path}/usr/local/bin
mkdir -p ${release_path}/usr/local/man/man1
mkdir -p ${release_path}/var/lib/nanocl
mkdir -p ${release_path}/etc

# Create and Copy release binary
cargo build --bin nanocld --release
cp ./target/release/${pkg_name} ${release_path}/usr/local/bin

# Generate DEBIAN control
cat > ${release_path}/DEBIAN/control <<- EOM
Package: ${pkg_name}
Version: ${version}
Architecture: ${arch}
Maintainer: next-hat team@next-hat.com
Description: A self-sufficient vms and containers manager
EOM

# cat > ${release_path}/DEBIAN/postinst <<- EOM
# getent group $1 > /dev/null 2&>1

# if [ $? -eq 0 ]; then
#     echo "nanocl group exist skipping"
# else
#     groupadd nanocl
#     echo "nanocl group created"
# fi
# EOM

# chmod 775 ${release_path}/DEBIAN/postinst

# cat > ${release_path}/DEBIAN/postrm <<- EOM
# EOM

# chmod 775 ${release_path}/DEBIAN/postrm

mkdir -p ./target/debian
dpkg-deb --build --root-owner-group ${release_path} ./target/debian/${pkg_name}_${version}_${arch}.deb
