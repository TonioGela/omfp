#!/usr/bin/env bash
# ENV-060: print the tagged version's dated section from CHANGELOG.md.
# Run from the checked-out release commit; no git history is needed.
set -euo pipefail

tag=${1:?usage: notes.sh <tag>}
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
version=${tag#v}
if [[ $tag == "$version" || -z $version ]]; then
    echo "Expected a version tag v<version>, got: $tag" >&2
    exit 1
fi

heading="## $version — "
active=false
has_content=false
notes=
while IFS= read -r line || [[ -n $line ]]; do
    if [[ $line == '## '* ]]; then
        if $active; then
            break
        fi
        if [[ $line == "$heading"* && ${line#"$heading"} =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
            active=true
        fi
    elif $active; then
        notes+="$line"$'\n'
        if [[ $line != '#'* && $line == *[![:space:]]* ]]; then
            has_content=true
        fi
    fi
done < "$root/CHANGELOG.md"

if ! $has_content; then
    echo "CHANGELOG.md needs a non-empty dated section for $tag: ## $version — YYYY-MM-DD" >&2
    exit 1
fi
printf '%s' "$notes"
