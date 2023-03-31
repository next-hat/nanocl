#!/bin/sh
## name: generate_man.sh

for file in ./target/man/*; do
  file_name=$(basename "${file}")
  pandoc --from man --to markdown < $file > ./doc/man/${file_name%.1}.md
done

echo "# Man Page\n## Summary" > ./doc/man/readme.md


for file in ./doc/man/*; do
  file_name=$(basename "${file}")
  echo "* [${file_name}](./${file_name})" >> ./doc/man/readme.md
done
