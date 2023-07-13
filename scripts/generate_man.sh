#!/bin/sh
## name: generate_man.sh

for project in ./bin/*; do

  if [ ! -d "$project/target/man" ]; then
    continue
  fi

  for file in $project/target/man/*; do
    file_name=$(basename "${file}")
    pandoc --from man --to markdown < $file > ./doc/man/${file_name%.1}.md
  done
done

echo "# Man Page\n## Summary" > ./doc/man/readme.md

for file in ./doc/man/*; do
  file_name=$(basename "${file}")
  echo "* [${file_name}](./${file_name})" >> ./doc/man/readme.md
done
