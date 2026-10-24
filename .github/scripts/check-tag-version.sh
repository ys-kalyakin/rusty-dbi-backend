#!/usr/bin/env bash
# Fail unless the invoking tag matches the workspace version.
# Takes the tag from $1 or $GITHUB_REF_NAME; the version from the first
# `version = "..."` line of the root Cargo.toml ([workspace.package]).
set -euo pipefail

tag="${1:-${GITHUB_REF_NAME:-}}"
if [[ -z "$tag" ]]; then
  echo "usage: $0 <tag> (or set GITHUB_REF_NAME)" >&2
  exit 2
fi

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n1)"
if [[ -z "$version" ]]; then
  echo "could not read the workspace version from Cargo.toml" >&2
  exit 2
fi

if [[ "v$version" == "$tag" ]]; then
  echo "tag $tag matches workspace version $version"
else
  echo "tag $tag does not match workspace version $version (expected v$version)" >&2
  exit 1
fi
