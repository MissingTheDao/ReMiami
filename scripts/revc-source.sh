#!/usr/bin/env bash
# Builds revc's C++ tree (cpp/re3-miami) from the player's own copy of the reVC source.
#
# The revc repository does not contain the reVC (re3-miami) source. It holds only revc's own
# work in cpp/revc/:
#   reVC.sha256   the reVC files revc needs, with their hashes (snapshot below)
#   patches/      revc's changes to those files (unified diffs)
#   overlay/      files that are revc's own (new C++ files, the vendored librw)
#
# Usage:
#   scripts/revc-source.sh prepare <copy> [--force]
#       <copy> is a folder, a .zip / .tar.* archive, or a git URL of the reVC source at the
#       snapshot below. Checks every needed file against reVC.sha256, then writes
#       cpp/re3-miami = original files + patches + overlay. Never downloads anything by itself.
#   scripts/revc-source.sh export
#       After editing cpp/re3-miami: writes the edits back into cpp/revc/patches and overlay.
#   scripts/revc-source.sh check
#       Exit 1 if cpp/re3-miami has edits that are not exported yet.
#
# REVC_CPP_DIR overrides the tree location (the build reads the same variable).
set -euo pipefail

SNAPSHOT=dc2147268b17b86656b629d5370bf2e90229e618   # re3 "miami" branch, 2021-09-02
REPO=$(cd "$(dirname "$0")/.." && pwd)
KIT=$REPO/cpp/revc
MANIFEST=$KIT/reVC.sha256
TREE=${REVC_CPP_DIR:-$REPO/cpp/re3-miami}
ORIG=$TREE/.reVC-original          # verified original files, kept for export (not part of the build)
STAMP=$TREE/.revc-source
DIFF=/usr/bin/diff

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
newtmp() { mktemp -d -p "$WORK"; }

die() { echo "revc-source: $*" >&2; exit 1; }
paths() { cut -c67- "$MANIFEST"; }

# Patch group for a path: src/core/X.cpp -> src-core, CMakeLists.txt -> root.
group() {
    case $1 in
        src/*/*) local g=${1#src/}; echo "src-${g%%/*}" ;;
        *) echo root ;;
    esac
}

# Files of the tree that are revc's own (not in the manifest). Generated files are skipped.
own_files() {
    (cd "$TREE" && find src cmake vendor/librw CMakeLists.txt -type f 2>/dev/null) \
        | /usr/bin/grep -v -e '^src/extras/GitSHA1.cpp$' -e '^src/extras/shaders/obj/' \
        | sort | comm -23 - <(paths | sort)
}

# Write patches/ and overlay/ for the current tree into $1.
export_to() {
    local out=$1 p g rc
    [[ -d $ORIG ]] || die "no original files in $ORIG; run 'prepare' first"
    rm -rf "$out/patches" "$out/overlay"
    mkdir -p "$out/patches" "$out/overlay"
    while IFS= read -r p; do
        g=$(group "$p")
        if [[ -f $TREE/$p ]]; then
            cmp -s "$ORIG/$p" "$TREE/$p" && continue
            rc=0; $DIFF -u --label "a/$p" --label "b/$p" "$ORIG/$p" "$TREE/$p" >> "$out/patches/$g.patch" || rc=$?
        else
            rc=0; $DIFF -u --label "a/$p" --label /dev/null "$ORIG/$p" /dev/null >> "$out/patches/$g.patch" || rc=$?
        fi
        [[ $rc -le 1 ]] || die "diff failed on $p"
    done < <(paths)
    if /usr/bin/grep -l '^Binary files ' "$out"/patches/*.patch 2>/dev/null; then
        die "a binary reVC file was changed; patches can't hold it"
    fi
    own_files | while IFS= read -r p; do
        mkdir -p "$out/overlay/$(dirname "$p")"
        cp -p "$TREE/$p" "$out/overlay/$p"
    done
}

cmd_check() {
    local tmp; tmp=$(newtmp)
    export_to "$tmp"
    if $DIFF -rq "$tmp/patches" "$KIT/patches" >/dev/null && $DIFF -rq "$tmp/overlay" "$KIT/overlay" >/dev/null; then
        echo "$TREE matches cpp/revc"
        return 0
    fi
    echo "$TREE has edits not in cpp/revc (run: scripts/revc-source.sh export):"
    $DIFF -rq "$tmp/patches" "$KIT/patches" | sed "s|$tmp/||; s|$KIT/||g" || true
    $DIFF -rq "$tmp/overlay" "$KIT/overlay" | sed "s|$tmp/||; s|$KIT/||g" || true
    return 1
}

cmd_export() {
    export_to "$KIT"
    echo "exported: $(ls "$KIT/patches" | wc -l) patch files ($(cat "$KIT"/patches/*.patch | /usr/bin/grep -c '^+++ ') files changed), $(find "$KIT/overlay" -type f | wc -l) own files"
}

cmd_prepare() {
    local src=${1:-} force=${2:-} tmp root base
    [[ -n $src ]] || die "usage: prepare <reVC source folder | archive | git URL> [--force]"
    tmp=$(newtmp)
    case $src in
        http://*|https://*|git@*|*.git)
            echo "cloning $src at $SNAPSHOT"
            git clone --quiet --filter=blob:none --no-checkout "$src" "$tmp/git"
            git -C "$tmp/git" checkout --quiet "$SNAPSHOT" || die "the repository has no commit $SNAPSHOT"
            base=$tmp/git ;;
        *.zip) unzip -q "$src" -d "$tmp/x"; base=$tmp/x ;;
        *.tar|*.tar.*|*.tgz) mkdir "$tmp/x"; tar -xf "$src" -C "$tmp/x"; base=$tmp/x ;;
        *) [[ -d $src ]] || die "$src is not a folder, archive or git URL"; base=$src ;;
    esac
    root=$(find "$base" -maxdepth 4 -path '*/src/core/Frontend.cpp' -print -quit)
    [[ -n $root ]] || die "no reVC source found in $src (looked for src/core/Frontend.cpp)"
    root=${root%/src/core/Frontend.cpp}

    echo "checking $(paths | wc -l) files against cpp/revc/reVC.sha256"
    local bad
    bad=$(cd "$root" && sha256sum --quiet -c "$MANIFEST" 2>/dev/null) || {
        echo "$bad" | head -20 >&2
        die "this copy differs from the reVC snapshot revc needs ($SNAPSHOT, $(echo "$bad" | wc -l) files wrong or missing)"
    }

    if [[ -e $TREE ]]; then
        if [[ $force != --force ]]; then
            [[ -f $STAMP ]] || die "$TREE exists and was not made by this script; move it away or use --force"
            cmd_check >/dev/null || die "$TREE has edits not exported to cpp/revc; run 'export' first or use --force"
        fi
        rm -rf "$TREE"
    fi
    mkdir -p "$ORIG"
    (cd "$root" && paths | xargs -d '\n' cp --parents -p -t "$ORIG")
    (cd "$ORIG" && paths | xargs -d '\n' cp --parents -p -t "$TREE")
    local p
    for p in "$KIT"/patches/*.patch; do
        patch -p1 -E --batch --forward --quiet -d "$TREE" < "$p" || die "patch $(basename "$p") failed"
    done
    cp -a "$KIT/overlay/." "$TREE/"
    echo "snapshot $SNAPSHOT, revc $(git -C "$REPO" rev-parse --short HEAD 2>/dev/null || echo '?')" > "$STAMP"
    echo "ready: $TREE"
}

case ${1:-} in
    prepare) shift; cmd_prepare "$@" ;;
    export) cmd_export ;;
    check) cmd_check ;;
    *) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
