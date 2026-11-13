#!/usr/bin/env bash
# ENV-060: the release notes of the tag `$1` from its Conventional Commits since the previous tag (all of them for the
# first one), grouped by type, as Markdown on stdout. Needs the full history (`fetch-depth: 0`).
set -euo pipefail

tag=${1:?usage: notes.sh <tag>}
previous=$(git describe --tags --abbrev=0 "$tag^" 2> /dev/null || true)
range=${previous:+$previous..}$tag

section() {
    local title=$1 pattern=$2 lines
    lines=$(git log --no-merges --format=%s "$range" | grep -E "$pattern" || true)
    if [[ -n $lines ]]; then
        printf '## %s\n\n' "$title"
        sed 's/^/- /' <<< "$lines"
        echo
    fi
}

section Features '^feat(\(.*\))?!?:'
section Fixes '^fix(\(.*\))?!?:'
section Other '^(perf|refactor|docs|test|build|ci|chore|style|revert)(\(.*\))?!?:'
if [[ -n $previous ]]; then
    echo "Commits: https://github.com/${GITHUB_REPOSITORY:-aciceri/omfp}/compare/$previous...$tag"
fi
