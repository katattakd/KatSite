#!/bin/bash
set -euo pipefail
STDIN=$(cat)

[ ! -f "style.css" ] && echo "body{font:16px/1.7 monospace;margin:auto;padding:1em 4em;max-width:64em;color:#ddd;background-color:#282a36}code,td,th{background-color:#222}code{padding:.2em .4em;font-size:95%;display:inline-block}a{color:#8ef;text-decoration:none}img{max-width:100%}blockquote{border-left:.4em solid #000;padding:0 1em;margin:0}table{border-collapse:collapse}td,th{padding:1em;border:.3em solid #000}" > "style.css"
printf "<link rel=stylesheet href=style.css>\n\n"

echo "$STDIN"
