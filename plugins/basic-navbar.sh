#!/bin/bash
set -euo pipefail
STDIN=$(cat)

[ "$1" != "markdown" ] && exit

for file in *.md; do
	document="${file%.*}"
	echo "[$document]($document.html)"
done

printf "\n---\n\n"

echo "$STDIN"
