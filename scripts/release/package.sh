#!/bin/sh
set -eu

REF="HEAD"
OUT_DIR="dist"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --ref)
      [ "$#" -ge 2 ] || { echo "--ref requires a value" >&2; exit 1; }
      REF="$2"
      shift 2
      ;;
    --out-dir)
      [ "$#" -ge 2 ] || { echo "--out-dir requires a value" >&2; exit 1; }
      OUT_DIR="$2"
      shift 2
      ;;
    -h|--help)
      echo "Usage: scripts/release/package.sh [--ref <git-ref>] [--out-dir <dir>]"
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ -n "$VERSION" ] || { echo "Could not read version from Cargo.toml" >&2; exit 1; }

git rev-parse --verify "$REF^{commit}" >/dev/null

mkdir -p "$OUT_DIR"
ARCHIVE="macmop-v${VERSION}.tar.gz"
ARCHIVE_PATH="${OUT_DIR}/${ARCHIVE}"
SHA_PATH="${ARCHIVE_PATH}.sha256"

git archive \
  --format=tar.gz \
  --prefix="macmop-v${VERSION}/" \
  -o "$ARCHIVE_PATH" \
  "$REF"

(
  cd "$OUT_DIR"
  shasum -a 256 "$ARCHIVE" > "${ARCHIVE}.sha256"
)

SHA=$(sed 's/ .*//' "$SHA_PATH")

echo "Archive: $ARCHIVE_PATH"
echo "Checksum: $SHA_PATH"
echo "SHA256: $SHA"
echo "Homebrew: sha256 \"$SHA\""
