#!/bin/sh
## name: generate_man.sh

# variables
pkg_name="nanocl"
arch=`dpkg --print-architecture`
version=`cat ./Cargo.toml | grep -m 1 "version = \"" | sed 's/[^0-9.]*\([0-9.]*\).*/\1/'`
release_path="./target/${pkg_name}_${version}_${arch}"

for file in ./target/man/*; do
  file_name=`basename ${file}`
  pandoc --from man --to markdown < $file > ./doc/${file_name%.1}.md
done
