#!/bin/sh
# tauri's deb bundler writes the builder's UID into the ar header; domain UIDs (9 digits) overflow it and dpkg rejects the file
set -e
cd "$(dirname "$0")/../src-tauri"
cargo tauri build --bundles deb
stage=$(ls -d target/release/bundle/deb/*/ | head -1)
root=$(mktemp -d)
cp -a "$stage/data/." "$root/"
mkdir "$root/DEBIAN" && cp "$stage/control/control" "$stage/control/md5sums" "$root/DEBIAN/"
deb="target/release/bundle/deb/$(basename "$stage").deb"
dpkg-deb --root-owner-group --build "$root" "$deb"
rm -rf "$root"
echo "$deb"
