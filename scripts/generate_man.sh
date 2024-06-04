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

echo """# Man Page

This is a collection of man pages for the projects in this repository.
It's only include man for the \`nanocl\` binary.
It is generated using \`pandoc\`.
With \`scripts/generate_man.sh\` script.

## Summary
""" > ./doc/man/readme.md

for file in $(ls -v ./doc/man/*); do
  file_name=$(basename "${file}")
  if [ "$file_name" = "readme.md" ]; then
    continue
  fi
  echo "* [${file_name}](./${file_name})" >> ./doc/man/readme.md
done
