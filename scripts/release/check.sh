#!/bin/sh
set -eu

export MACMOP_TEST_MODE=1
export MACMOP_DATA_DIR="${MACMOP_DATA_DIR:-/private/tmp/macmop-release-check-data}"
export MACMOP_TRASH_DIR="${MACMOP_TRASH_DIR:-/private/tmp/macmop-release-check-trash}"
export MACMOP_AUDIT_FILE="${MACMOP_AUDIT_FILE:-${MACMOP_DATA_DIR}/audit/last.json}"
export MACMOP_ROLLBACK_FILE="${MACMOP_ROLLBACK_FILE:-${MACMOP_DATA_DIR}/rollback/entries.json}"

git diff --check
git status --short
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release

if [ -n "$(git status --short)" ]; then
  cargo package --allow-dirty --offline --locked
else
  cargo package --offline --locked
fi

if command -v ruby >/dev/null 2>&1; then
  ruby -c Formula/macmop.rb
fi
