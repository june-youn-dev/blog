#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "==> API checks"
(
  cd "${repo_root}/apps/api"
  pnpm run check
)

echo
echo "==> Site checks"
(
  cd "${repo_root}/apps/site"
  pnpm run check
)

echo
echo "All checks passed."
