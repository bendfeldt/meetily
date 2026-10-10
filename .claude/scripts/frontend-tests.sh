#!/usr/bin/env bash
# Run meetily's frontend tests the way they pass upstream: one Bun process per
# file (Bun shares module mocks across files, so a single `bun test` run leaks
# mocks and fails ~19 tests), plus node:test for the .mjs suites.
# Fork-owned helper: upstream's package.json has no test script.
set -euo pipefail

cd "$(dirname "$0")/../../frontend"

fail=0
while IFS= read -r f; do
  bun test "./$f" || fail=1
done < <(find tests \( -name '*.test.ts' -o -name '*.test.tsx' -o -name '*.test.js' \) | sort)

mjs=$(find tests -name '*.test.mjs' | sort)
if [ -n "$mjs" ]; then
  # shellcheck disable=SC2086
  node --test $mjs || fail=1
fi

exit "$fail"
