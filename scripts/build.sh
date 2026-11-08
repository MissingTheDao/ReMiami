#!/usr/bin/env bash
# Reproducible build of reVC (Rust host + C++ game), from a clean clone.
# usage: scripts/build.sh [--strict]
#   --strict  fail (instead of warn) if the toolchain or system packages differ
#             from rust/rust-toolchain.toml and scripts/deps.lock
# Output: rust/target/release/revc  (run it with REVC_GAME_DIR=<your VC folder>,
#         or from a checkout that has the game copy in ./game)
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
STRICT=0; [[ "${1:-}" == "--strict" ]] && STRICT=1
problems=0
warn() { printf '\033[1;33m!! %s\033[0m\n' "$*" >&2; problems=$((problems+1)); }

# Toolchain
want=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$ROOT/rust/rust-toolchain.toml")
have=$(rustc --version | awk '{print $2}')
[[ "$have" == "$want" ]] || warn "rustc $have, expected $want (rust/rust-toolchain.toml)"

# System packages
if command -v pacman >/dev/null; then
  while read -r pkg ver; do
    [[ -z "$pkg" || "$pkg" == \#* ]] && continue
    cur=$(pacman -Q "$pkg" 2>/dev/null | awk '{print $2}') || true
    if [[ -z "$cur" ]]; then warn "package $pkg missing (want $ver)"
    elif [[ "$cur" != "$ver" ]]; then warn "package $pkg is $cur, deps.lock has $ver"; fi
  done < "$ROOT/scripts/deps.lock"
else
  warn "pacman not found; cannot check scripts/deps.lock"
fi
if (( problems > 0 && STRICT )); then echo "refusing to build (--strict)" >&2; exit 1; fi

# Path-independent output: map this checkout and the cargo home out of the binary.
CARGO_HOME_DIR=${CARGO_HOME:-$HOME/.cargo}
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$ROOT=revc --remap-path-prefix=$CARGO_HOME_DIR=cargo"
# sccache is fine for speed but keep it out of the picture for the reference build.
export RUSTC_WRAPPER=

cd "$ROOT/rust"
cargo build --release --locked
bin="${CARGO_TARGET_DIR:-$ROOT/rust/target}/release/revc"
echo
echo "built: $bin"
sha256sum "$bin"
(( problems == 0 )) || echo "($problems warning(s) above: output may differ from the reference build)"
