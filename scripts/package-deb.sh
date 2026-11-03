#!/bin/sh
# tauri's deb bundler writes the builder's UID into the ar header; domain UIDs (9 digits) overflow it and dpkg rejects the file
set -e
cd "$(dirname "$0")/../src-tauri"
# updater artifacts need the signing key; CI passes it as secrets, a local build reads ~/.tauri
if [ -z "$TAURI_SIGNING_PRIVATE_KEY" ] && [ -f "$HOME/.tauri/muttei.key" ]; then
  TAURI_SIGNING_PRIVATE_KEY=$(cat "$HOME/.tauri/muttei.key")
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$(cat "$HOME/.tauri/muttei.key.password" 2>/dev/null)
  export TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD
fi
${TAURI:-cargo tauri} build --bundles deb
stage=$(ls -td target/release/bundle/deb/*/ | head -1)
root=$(mktemp -d)
cp -a "$stage/data/." "$root/"
mkdir "$root/DEBIAN" && cp "$stage/control/control" "$stage/control/md5sums" "$root/DEBIAN/"
deb="target/release/bundle/deb/$(basename "$stage").deb"
dpkg-deb --root-owner-group --build "$root" "$deb" >&2
rm -rf "$root"
# the repack changed the bytes, so the bundler's signature no longer matches
${TAURI:-cargo tauri} signer sign "$deb" >&2
echo "$deb"
