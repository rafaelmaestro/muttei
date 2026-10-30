#!/bin/sh
# zip for the Chrome Web Store: manifest at the root, nothing but the files the extension loads
set -e
cd "$(dirname "$0")/.."
v=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' extension/manifest.json)
mkdir -p dist
out="dist/muttei-extension-$v.zip"
rm -f "$out"
(cd extension && zip -q -X "../$out" manifest.json background.js content.js icon*.png)
echo "$out"
