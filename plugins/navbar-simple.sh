#!/bin/bash
set -euo pipefail
STDIN=$(cat)

[ "$1" != "markdown" ] && exit

printf "\n[Home](index.html)\n"
for file in *.md; do
	document="${file%.*}"
	[[ "$document" != "index" ]] && echo "[$document]($document.html)"
done

printf "\n---\n\n"

echo "$STDIN"
