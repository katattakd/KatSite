#!/bin/bash
set -euo pipefail
STDIN=$(cat)

[ "$1" == "markdown" ] && echo "$STDIN"
[ "$1" != "postinit" ] && exit

type html-minifier-terser >/dev/null 2>&1 || {
	printf "\nThe minifier plugin requires html-minifier-terser to be installed!
To install it, run the below command:
	npm install html-minifier-terser -g\n\n" > /dev/stderr
	exit 1
}

html-minifier-terser \
	--case-sensitive \
	--collapse-boolean-attributes \
	--collapse-whitespace \
	--decode-entities \
	--no-include-auto-generated-tags \
	--minify-css true \
	--minify-js true \
	--process-conditional-comments \
	--remove-attribute-quotes \
	--remove-comments \
	--remove-empty-attributes \
	--remove-empty-elements \
	--remove-optional-tags \
	--remove-redundant-attributes \
	--remove-script-type-attributes \
	--remove-style-link-type-attributes \
	--sort-attributes \
	--sort-class-name \
	--trim-custom-fragments \
	--use-short-doctype \
	--input-dir . \
	--output-dir tmp \
	--file-ext html

mv tmp/* .
rm -r tmp
