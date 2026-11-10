#!/usr/bin/env bash
# The Dutch manual (docs/handleiding) and the English one (docs/manual) are kept in
# step by hand. This checks the structure, not the wording: every chapter exists in
# both, with the same number of headings per level, the same images in the same
# order and the same number of code fences. Run from the repository root; CI runs it.
set -euo pipefail

nl=docs/handleiding
en=docs/manual
fail=0

shape() {
    # heading levels in order, image references in order, number of code fences
    grep -E '^#{1,6} ' "$1" | sed -E 's/^(#+) .*/\1/' | tr '\n' ' '
    echo
    grep -oE '\(\.\./images/[^)]+\)' "$1" || true
    echo "fences: $(grep -cE '^```' "$1" || true)"
}

for f in "$nl"/*.md; do
    base=$(basename "$f")
    prefix=${base%%-*}                 # "03" of "03-gui.md"; "README.md" has no dash
    if [ "$prefix" = "$base" ]; then
        other="$en/$base"
    else
        other=$(ls "$en"/"$prefix"-*.md 2>/dev/null | head -1 || true)
    fi
    if [ -z "$other" ] || [ ! -f "$other" ]; then
        echo "MISSING: $f has no counterpart in $en"
        fail=1
        continue
    fi
    if ! diff <(shape "$f") <(shape "$other") > /dev/null; then
        echo "DIFFERENT STRUCTURE: $f  <->  $other"
        diff <(shape "$f") <(shape "$other") || true
        fail=1
    fi
done

for f in "$en"/*.md; do
    base=$(basename "$f")
    prefix=${base%%-*}
    if [ "$prefix" != "$base" ] && ! ls "$nl"/"$prefix"-*.md > /dev/null 2>&1; then
        echo "MISSING: $f has no counterpart in $nl"
        fail=1
    fi
done

if [ "$fail" -eq 0 ]; then
    echo "manuals in step: $(ls "$nl"/*.md | wc -l | tr -d ' ') files each"
fi
exit "$fail"
