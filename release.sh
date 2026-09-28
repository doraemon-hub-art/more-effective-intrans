#!/usr/bin/env bash
#/**
# * @file release.sh
# * @author doraemon-hub-art (1660219734@qq.com)
# * @brief Cut a release: bump the version, build the package and publish it on GitHub
# * @date 2026-09-27
# *
# * @copyright Copyright (c) 2026
# */

# Usage:
#   ./release.sh patch|minor|major|<x.y.z> [options]
#   ./release.sh --no-bump [options]
#
#   --dry-run        print the plan and change nothing
#   --yes            do not ask before pushing and publishing
#   --no-push        stop after the tag; print the commands to push and publish by hand
#   --no-bump        release the version the manifest already carries: nothing is bumped and
#                    nothing is committed. The tag is used if it is there and created if it is
#                    not, which is what the very first release needs
#   --notes-file F   use a file as the release notes (default: GitHub generates them, listing
#                    the commits since the previous release)
#   --root DIR       release the checkout in DIR (default: the directory holding this script)
#
# Without --no-bump it does, in order: check the version → rewrite Cargo.toml → cargo test and
# cargo deb in the build container → git commit → git tag → git push → gh release create with the
# .deb attached. With --no-bump the version is left as it stands, and the tag has to be there.
#
# The package is built in the container from Dockerfile.release, on Jammy, so that the binary asks
# for no glibc newer than 2.35 — see that file for why.
#
# It stops instead of guessing: a dirty tree, an existing tag, a version that does not go up, or
# a `gh` that is missing or not logged in are all reasons to refuse before anything is touched.
set -euo pipefail

LEVEL=""
DRY_RUN=0
ASSUME_YES=0
PUSH=1
NO_BUMP=0
NOTES_FILE=""
ROOT="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"

while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) DRY_RUN=1 ;;
        --yes) ASSUME_YES=1 ;;
        --no-push) PUSH=0 ;;
        --no-bump) NO_BUMP=1 ;;
        --notes-file) NOTES_FILE="${2:?--notes-file needs a path}"; shift ;;
        --root) ROOT="${2:?--root needs a path}"; shift ;;
        -h|--help) sed -n '/^# Usage:/,/^set -euo pipefail/p' "$0" | sed '$d'; exit 0 ;;
        -*) echo "unknown option: $1" >&2; exit 2 ;;
        *) LEVEL="$1" ;;
    esac
    shift
done

[ -n "$LEVEL" ] || [ "$NO_BUMP" -eq 1 ] \
    || { echo "usage: ./release.sh patch|minor|major|<x.y.z> | --no-bump" >&2; exit 2; }

MANIFEST="$ROOT/Cargo.toml"
cd "$ROOT"
[ -f "$MANIFEST" ] || { echo "no Cargo.toml in $ROOT" >&2; exit 2; }

step() { printf '\n=== %s\n' "$*"; }
run() {
    printf '    %s\n' "$*"
    if [ "$DRY_RUN" -eq 0 ]; then "$@"; fi
}
confirm() {
    # Nothing is at stake in a dry run, so there is nothing to confirm either.
    [ "$DRY_RUN" -eq 1 ] && return 0
    [ "$ASSUME_YES" -eq 1 ] && return 0
    printf '\n%s [y/N] ' "$1"
    read -r answer
    case "$answer" in [yY]*) return 0 ;; *) echo "stopped."; exit 1 ;; esac
}

# ---------------------------------------------------------------- checks

step "checking the ground"
current="$(awk '/^\[package\]/{seen=1} seen && /^version *=/{gsub(/[" ]/,"",$3); print $3; exit}' "$MANIFEST")"
[ -n "$current" ] || { echo "could not read the version out of $MANIFEST" >&2; exit 1; }

case "$LEVEL" in
    patch|minor|major)
        IFS=. read -r major minor patch <<< "$current"
        case "$LEVEL" in
            patch) next="$major.$minor.$((patch + 1))" ;;
            minor) next="$major.$((minor + 1)).0" ;;
            major) next="$((major + 1)).0.0" ;;
        esac
        ;;
    "") next="$current" ;;
    *)
        next="$LEVEL"
        [[ "$next" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "not a version: $next" >&2; exit 2; }
        ;;
esac

if [ "$NO_BUMP" -eq 1 ]; then
    # The version is whatever the manifest says. Its tag is used when it is there — cargo release
    # having written both — and created here when it is not, which is the first release of all.
    [ "$next" = "$current" ] || { echo "--no-bump takes no level or version" >&2; exit 2; }
    tag="v$next"
    if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
        echo "    version $current (from the manifest), tag $tag (already there)"
    else
        echo "    version $current (from the manifest), tag $tag (to be created on HEAD)"
        create_tag=1
    fi
else
    # A version that does not go up would make a tag that clashes or an upload that overwrites.
    highest="$(printf '%s\n%s\n' "$current" "$next" | sort -V | tail -1)"
    [ "$highest" = "$next" ] && [ "$next" != "$current" ] \
        || { echo "$next does not go up from $current" >&2; exit 1; }

    tag="v$next"
    git rev-parse -q --verify "refs/tags/$tag" >/dev/null \
        && { echo "$tag already exists" >&2; exit 1; }

    echo "    version $current -> $next, tag $tag"
fi

if [ -n "$(git status --porcelain)" ]; then
    echo "the tree is dirty: commit or stash first, this script makes a commit of its own" >&2
    exit 1
fi

if [ "$PUSH" -eq 1 ]; then
    command -v gh >/dev/null || { echo "gh is not installed (sudo apt install gh)" >&2; exit 1; }
    gh auth status >/dev/null 2>&1 || { echo "gh is not logged in (gh auth login)" >&2; exit 1; }
fi

command -v docker >/dev/null || { echo "docker is not installed (sudo apt install docker.io)" >&2; exit 1; }

# ---------------------------------------------------------------- bump and build

if [ "$NO_BUMP" -eq 0 ]; then
    step "bumping the version"
    if [ "$DRY_RUN" -eq 0 ]; then
        python3 - "$MANIFEST" "$next" <<'PY'
import sys
path, new = sys.argv[1], sys.argv[2]
lines = open(path).read().splitlines(keepends=True)
seen = False
for index, line in enumerate(lines):
    if line.startswith("[package]"):
        seen = True
    elif seen and line.startswith("version ="):
        lines[index] = f'version = "{new}"\n'
        break
else:
    raise SystemExit("no version line under [package]")
open(path, "w").writelines(lines)
PY
    fi
    run grep -n '^version' "$MANIFEST"
else
    step "the version and the tag are cargo release's, so they are left alone"
    run grep -n '^version' "$MANIFEST"
fi

# ---------------------------------------------------------------- the build container

# Every symbol the binary imports is bound to the glibc version of the machine that links it, so a
# package built with the toolchain of this machine asks for GLIBC_2.38 and GLIBC_2.39 and will not
# start on Jammy. It is built on Jammy — glibc 2.35, the oldest distribution it is meant to run on
# — inside the container from Dockerfile.release. Glibc only goes forwards, so that one package
# covers Jammy and everything newer; there is no package per distribution.
IMAGE="more-effective-intrans/release:22.04"
DEB_TARGET="target/jammy"

step "preparing the build container"
docker image inspect "$IMAGE" >/dev/null 2>&1 \
    || run docker build -f "$ROOT/Dockerfile.release" -t "$IMAGE" "$ROOT"

step "running the tests and building the package"
if [ "$DRY_RUN" -eq 0 ]; then
    # The directory is made here rather than by the container: the build runs as root, and a target
    # directory created by root in the checkout could not be cleaned up by hand afterwards.
    mkdir -p "$ROOT/$DEB_TARGET"
    docker run --rm -i -v "$ROOT":/work -w /work \
        -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" "$IMAGE" bash -eu <<'SH'
export CARGO_TARGET_DIR=/work/target/jammy
# The build runs as root, the files it writes have to belong to the person who invoked it.
trap 'chown -R "$HOST_UID:$HOST_GID" "$CARGO_TARGET_DIR" 2>/dev/null || true' EXIT

# The rpath goes to the linker instead of to patchelf afterwards: cargo deb builds the binary once
# more, and a relink would quietly drop a patch applied before it. $ORIGIN is where the binary
# itself sits, usr/bin, and the libraries are installed one level up from there.
export RUSTFLAGS='-C link-arg=-Wl,-rpath,$ORIGIN/../lib/more-effective-intrans/lib'

cargo test --quiet
cargo build --release --quiet

# The runtime libraries travel inside the package, so that nothing but libc6 has to come from the
# machine: fontconfig with what it needs, and the unwinder Rust links against. They are taken out
# of this container — glibc 2.35 — for the same reason the binary is built in it.
libs="$CARGO_TARGET_DIR/libs"
rm -rf "$libs"
mkdir -p "$libs"
pending=(libfontconfig.so.1 libgcc_s.so.1)
while [ "${#pending[@]}" -gt 0 ]; do
    name="${pending[0]}"
    pending=("${pending[@]:1}")
    [ -e "$libs/$name" ] && continue
    cp -L "/usr/lib/x86_64-linux-gnu/$name" "$libs/"
    for dep in $(ldd "$libs/$name" | awk '$2 == "=>" {print $1}'); do
        case "$dep" in
            libc.so.6 | libm.so.6 | ld-linux-x86-64.so.2) ;;
            *) pending+=("$dep") ;;
        esac
    done
done

# Each copied library is pointed at its own neighbours, so that the ones shipped together are the
# ones that get used rather than whatever version the machine happens to carry.
for lib in "$libs"/*.so*; do
    patchelf --set-rpath '$ORIGIN' "$lib"
done

cargo deb
SH
else
    echo "    docker run --rm -i -v $ROOT:/work -w /work $IMAGE bash -eu <<'SH'"
    echo "    ... cargo test, cargo build --release, stage the libraries, patchelf, cargo deb"
fi

# The package name follows the crate version: with and without a Debian revision are both matched,
# so turning the revision on or off in Cargo.toml does not need this line to change.
deb="$(ls -1 "$ROOT/$DEB_TARGET"/debian/more-effective-intrans_"$next"[-_]*.deb 2>/dev/null | tail -1 || true)"
if [ "$DRY_RUN" -eq 0 ]; then
    [ -n "$deb" ] || { echo "cargo deb produced no package for $next" >&2; exit 1; }
    echo "    package: $deb"
else
    echo "    package: $DEB_TARGET/debian/more-effective-intrans_$next*.deb"
fi

# ---------------------------------------------------------------- commit, tag, publish

if [ "$NO_BUMP" -eq 0 ]; then
    step "committing and tagging"
    run git add "$MANIFEST"
    run git commit -m "chore(release): $tag"
    run git tag -a "$tag" -m "$tag"
elif [ "${create_tag:-0}" -eq 1 ]; then
    step "tagging the commit that is already there"
    run git tag -a "$tag" -m "$tag"
fi

if [ "$PUSH" -eq 0 ]; then
    step "stopping before the push, as asked"
    cat <<EOF
    git push -u origin HEAD && git push origin $tag
    gh release create $tag --title "$tag" ${NOTES_FILE:+--notes-file "$NOTES_FILE"} $DEB_TARGET/debian/more-effective-intrans_$next*.deb
EOF
    exit 0
fi

confirm "Push to the remote and publish $tag with the package attached?"
step "pushing"
# -u origin HEAD rather than a bare `git push`: a branch without an upstream makes the bare form
# fail with nothing pushed at all.
run git push -u origin HEAD
# A release that stopped halfway — the tag went up, the publishing did not — is picked up here
# again: the tag is only pushed when the remote does not have this very tag object already.
remote_tag="$(git ls-remote --tags origin "refs/tags/$tag" 2>/dev/null | awk '{print $1}')"
if [ "$remote_tag" = "$(git rev-parse "$tag" 2>/dev/null)" ]; then
    echo "    $tag is already on the remote, pointing at the same commit"
else
    run git push origin "$tag"
fi

step "publishing the release"
if [ -n "$NOTES_FILE" ]; then
    run gh release create "$tag" --title "$tag" --notes-file "$NOTES_FILE" "$deb"
else
    # --generate-notes lets GitHub write the body itself: it lists every commit since the previous
    # release and closes with a compare link, which is exactly "what changed since last time".
    run gh release create "$tag" --title "$tag" --generate-notes "$deb"
fi

step "done"
echo "    $tag is out, with $(basename "${deb:-$DEB_TARGET/debian/more-effective-intrans_$next*.deb}") attached"
