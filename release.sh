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
#   --notes-file F   use a file as the release notes (default: the commits since the last tag)
#   --root DIR       release the checkout in DIR (default: the directory holding this script)
#
# Without --no-bump it does, in order: check the version → rewrite Cargo.toml → cargo test →
# cargo deb → git commit → git tag → git push → gh release create with the .deb attached.
# With --no-bump the first three of those are left out, and the tag has to be there already.
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

step "running the tests"
run cargo test --quiet

step "building the package"
run cargo deb
# The package name follows the crate version: with and without a Debian revision are both matched,
# so turning the revision on or off in Cargo.toml does not need this line to change.
deb="$(ls -1 "$ROOT"/target/debian/more-effective-intrans_"$next"[-_]*.deb 2>/dev/null | tail -1 || true)"
if [ "$DRY_RUN" -eq 0 ]; then
    [ -n "$deb" ] || { echo "cargo deb produced no package for $next" >&2; exit 1; }
    echo "    package: $deb"
else
    echo "    package: target/debian/more-effective-intrans_$next*.deb"
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
    gh release create $tag --title "$tag" ${NOTES_FILE:+--notes-file "$NOTES_FILE"} target/debian/more-effective-intrans_$next*.deb
EOF
    exit 0
fi

confirm "Push to the remote and publish $tag with the package attached?"
step "pushing"
# -u origin HEAD rather than a bare `git push`: a branch without an upstream makes the bare form
# fail with nothing pushed at all.
run git push -u origin HEAD
run git push origin "$tag"

step "publishing the release"
if [ -n "$NOTES_FILE" ]; then
    run gh release create "$tag" --title "$tag" --notes-file "$NOTES_FILE" "$deb"
else
    notes="$(git log --oneline "$(git describe --tags --abbrev=0 "$tag^" 2>/dev/null || echo "$tag^")..HEAD~0" 2>/dev/null || true)"
    [ -n "$notes" ] || notes="Release $tag"
    if [ "$DRY_RUN" -eq 1 ]; then
        printf '    gh release create %s --title %s --notes <the commits since the last tag> %s\n' \
            "$tag" "$tag" "$deb"
    else
        tmp="$(mktemp)"
        printf '%s\n' "$notes" > "$tmp"
        gh release create "$tag" --title "$tag" --notes-file "$tmp" "$deb"
        rm -f "$tmp"
    fi
fi

step "done"
echo "    $tag is out, with $(basename "${deb:-target/debian/more-effective-intrans_$next*.deb}") attached"
