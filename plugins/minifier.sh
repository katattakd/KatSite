#!/bin/bash
set -euo pipefail
STDIN=$(cat)

[ "$1" != "html" ] && exit

type html-minifier-terser >/dev/null 2>&1 || {
	printf "\nThe minifier plugin requires html-minifier-terser to be installed!
To install it, run the below command:
	npm install html-minifier-terser -g\n\n" > /dev/stderr
	exit 1
}

echo "$STDIN" | html-minifier-terser \
	--case-sensitive \
	--collapse-boolean-attributes \
	--collapse-whitespace \
	--decode-entities \
	--no-include-auto-generated-tags \
	--minify-css \
	--minify-js \
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
	--use-short-doctype
