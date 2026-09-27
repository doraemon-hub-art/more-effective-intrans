#!/usr/bin/env bash
#/**
# * @file format.sh
# * @author doraemon-hub-art (1660219734@qq.com)
# * @brief Format every Rust file in the repository
# * @date 2026-09-27
# *
# * @copyright Copyright (c) 2026
# */

# `cargo fmt --all` runs rustfmt over everything the package builds from: the library, the binary,
# the build script, the tests and any examples. It is the same command the project's habit is to
# run before a commit, wrapped so it can be called from anywhere and always formats this tree.
set -euo pipefail

# Where this script lives, so that calling it from another directory still formats the repository
# it belongs to.
cd "$(dirname "$(readlink -f "$0")")"

cargo fmt --all
echo "formatted every Rust file under $(pwd)"
