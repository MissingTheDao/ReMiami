#!/usr/bin/env bash
# Packs the reMiami build kit AppImage: the repository at <commit> (git archive), its Rust crates vendored so the
# build needs no network, the launcher (AppRun). No game data, no reVC source, no prebuilt game.
# usage: packaging/buildkit/make-buildkit.sh <commit-or-tag> <version> <type2-runtime file> [out dir]
# Needs: git, cargo, mksquashfs (squashfs-tools), and the AppImage type2 runtime of the pinned release below
# (https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64).
# The crates are vendored for every target in Cargo.lock (cargo vendor --locked: exactly the locked versions and
# checksums; crates not yet in the local cargo cache are fetched from crates.io).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
REV=$1 VERSION=$2 RUNTIME=$3 OUT=${4:-$ROOT/dist}
command -v mksquashfs >/dev/null || { echo "mksquashfs not found (squashfs-tools)" >&2; exit 2; }
RUNTIME_SHA256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d	# type2-runtime 20251108, runtime-x86_64
echo "$RUNTIME_SHA256  $RUNTIME" | sha256sum -c --quiet - || { echo "the runtime is not the pinned 20251108 runtime-x86_64" >&2; exit 2; }
export SOURCE_DATE_EPOCH=$(git -C "$ROOT" log -1 --format=%ct "$REV")
W=$(mktemp -d -p "${TMPDIR:-/var/tmp}" remiami-buildkit.XXXXXX)
trap 'rm -r "$W"' EXIT
APP=$W/AppDir
mkdir -p "$APP/src"
git -C "$ROOT" archive --format=tar "$REV" | tar -x -C "$APP/src"
# Never package game data, imports or generated C++: refuse if anything of the kind is in the archive.
for d in game cpp/re3-miami ai logs tests/reference; do
  [[ ! -e $APP/src/$d ]] || { echo "refusing: $d is in the source archive" >&2; exit 1; }
done
# The crates, vendored (the build then runs with CARGO_NET_OFFLINE=true).
(cd "$APP/src/rust" && cargo vendor --locked --versioned-dirs vendor > "$W/vendor.toml")
mkdir -p "$APP/src/rust/.cargo"
sed -n '/^\[source/,$p' "$W/vendor.toml" > "$APP/src/rust/.cargo/config.toml"
echo "$VERSION" > "$APP/VERSION"
install -m755 "$ROOT/packaging/buildkit/AppRun" "$APP/AppRun"
cat > "$APP/remiami-buildkit.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=reMiami build kit
Exec=AppRun
Icon=remiami-buildkit
Categories=Game;
Terminal=true
DESK
cp "$ROOT/packaging/buildkit/remiami-buildkit.png" "$APP/remiami-buildkit.png"
ln -s remiami-buildkit.png "$APP/.DirIcon"
find "$APP" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
mksquashfs "$APP" "$W/fs.sqfs" -root-owned -noappend -comp zstd -Xcompression-level 19 -b 1M -no-xattrs -quiet	# (times from SOURCE_DATE_EPOCH)
mkdir -p "$OUT"
F=$OUT/remiami-buildkit-$VERSION-x86_64.AppImage
cat "$RUNTIME" "$W/fs.sqfs" > "$F"
chmod +x "$F"
ls -l "$F"; sha256sum "$F"
