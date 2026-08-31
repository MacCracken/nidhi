#!/bin/bash
# nidhi Version Bump Script
# SemVer: MAJOR.MINOR.PATCH
# Usage: ./bump-version.sh <new_version>
# Example: ./bump-version.sh 2.0.1
#
# VERSION is the single source of truth: cyrius.cyml derives it via
# version = "${file:VERSION}", and .github/workflows/release.yml verifies
# both against the pushed tag. So this script only writes VERSION.

set -e

if [ -z "$1" ]; then
    echo "Current version: $(tr -d '[:space:]' < VERSION)"
    echo "Usage: $0 <new_version>"
    echo "Example: $0 2.0.1"
    exit 1
fi

NEW_VERSION="$1"
OLD_VERSION=$(tr -d '[:space:]' < VERSION)

case "$NEW_VERSION" in
    [0-9]*.[0-9]*.[0-9]*) ;;
    *) echo "error: '$NEW_VERSION' is not MAJOR.MINOR.PATCH"; exit 1 ;;
esac

echo "Bumping version: $OLD_VERSION -> $NEW_VERSION"

echo "$NEW_VERSION" > VERSION

# cyrius.cyml must keep deriving from VERSION rather than inlining a number.
CYML_RAW=$(grep '^version = ' cyrius.cyml | head -1 | sed 's/version = "\(.*\)"/\1/')
if [ "$CYML_RAW" != '${file:VERSION}' ]; then
    echo "warning: cyrius.cyml pins version = \"$CYML_RAW\" instead of \${file:VERSION}"
fi

echo ""
echo "Updated files:"
echo "  VERSION     -> $NEW_VERSION"
echo "  cyrius.cyml -> $CYML_RAW (derived)"
echo ""
echo "Next: update CHANGELOG.md, then \`cyrius distlib\` to restamp dist/nidhi.cyr."
