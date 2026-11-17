#!/usr/bin/env bash
# Packs the reMiami build kit for Windows: <launcher .exe> + a .tar.gz of build.ps1, VERSION and the repository at
# <commit> with its Rust crates vendored for every target in Cargo.lock + a trailer (version, lengths, "RMKITTAR").
# No game data, no reVC source, no prebuilt game.
# usage: packaging/buildkit/make-buildkit-win.sh <commit-or-tag> <version> <remiami-kit.exe> [out dir]
#   <remiami-kit.exe>: packaging/buildkit/win-launcher built on Windows (cargo build --release, MSVC).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
REV=$1 VERSION=$2 LAUNCHER=$3 OUT=${4:-$ROOT/dist}
[[ -f $LAUNCHER ]] || { echo "launcher not found: $LAUNCHER" >&2; exit 2; }
export SOURCE_DATE_EPOCH=$(git -C "$ROOT" log -1 --format=%ct "$REV")
W=$(mktemp -d -p "${TMPDIR:-/var/tmp}" remiami-buildkit-win.XXXXXX)
trap 'rm -r "$W"' EXIT
K=$W/kit
mkdir -p "$K/src"
git -C "$ROOT" archive --format=tar "$REV" | tar -x -C "$K/src"
for d in game cpp/re3-miami ai logs tests/reference; do
  [[ ! -e $K/src/$d ]] || { echo "refusing: $d is in the source archive" >&2; exit 1; }
done
(cd "$K/src/rust" && cargo vendor --locked --versioned-dirs vendor > "$W/vendor.toml")
mkdir -p "$K/src/rust/.cargo"
sed -n '/^\[source/,$p' "$W/vendor.toml" > "$K/src/rust/.cargo/config.toml"
echo "$VERSION" > "$K/VERSION"
cp "$K/src/packaging/buildkit/build.ps1" "$K/build.ps1"
tar --sort=name --mtime="@$SOURCE_DATE_EPOCH" --owner=0 --group=0 --numeric-owner --format=gnu -C "$K" -cf - VERSION build.ps1 src \
  | gzip -9 -n > "$W/kit.tar.gz"
mkdir -p "$OUT"
F=$OUT/remiami-buildkit-$VERSION-windows-x86_64.exe
python3 -I - "$LAUNCHER" "$W/kit.tar.gz" "$VERSION" "$F" <<'PY'
import struct, sys
exe, kit, version, out = sys.argv[1:]
d = open(kit, 'rb').read(); v = version.encode()
with open(out, 'wb') as f:
    f.write(open(exe, 'rb').read()); f.write(d); f.write(v)
    f.write(struct.pack('<I', len(v))); f.write(struct.pack('<Q', len(d))); f.write(b'RMKITTAR')
PY
ls -l "$F"; sha256sum "$F"
