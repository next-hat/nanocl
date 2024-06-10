#!/bin/sh
## name: generate_man.sh

for project in ./bin/*; do

  if [ ! -d "$project/target/man" ]; then
    continue
  fi

  count=1
  for file in $(ls $project/target/man/*); do
    echo "Processing $file"
    file_name=$(basename "${file}")
    # replace - with _ in the file name
    # replace nanocl with nanocl_ in the file name
    title=$(echo ${file_name%.1} | sed 's/nanocl-//g' | sed 's/-/ /g' | sed 's/.*/\u&/')
    echo "---
title: ${title}
sidebar_position: ${count}
---

# ${title}
" > ./doc/man/${file_name%.1}.md
    pandoc --from man --to gfm < $file >> ./doc/man/${file_name%.1}.md
    awk '/^# NAME/{flag=1; next} /^# SYNOPSIS/{flag=0} !flag' ./doc/man/${file_name%.1}.md | sponge ./doc/man/${file_name%.1}.md
    # Replace # SYNOPSIS with ## SYNOPSIS
    sed -i "s/^# SYNOPSIS/## SYNOPSIS/" -i ./doc/man/${file_name%.1}.md
    # Replace # DESCRIPTION with ## DESCRIPTION
    sed -i "s/^# DESCRIPTION/## DESCRIPTION/" -i ./doc/man/${file_name%.1}.md
    # Replace # OPTIONS with ## OPTIONS
    sed -i "s/^# OPTIONS/## OPTIONS/" -i ./doc/man/${file_name%.1}.md
    count=$((count+1))
  done
done

echo """# Man Page

This is a collection of man pages for the projects in this repository.
It's only include man for the \`nanocl\` binary.
It is generated using \`pandoc\`.
With \`scripts/generate_man.sh\` script.

## Summary
""" > ./doc/man/readme.md

for file in $(ls ./doc/man/*); do
  echo "Processing $file"
  file_name=$(basename "${file}")
  if [ "$file_name" = "readme.md" ]; then
    continue
  fi
  echo "* [${file_name}](./${file_name})" >> ./doc/man/readme.md
done
