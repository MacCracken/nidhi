#!/bin/bash
# nidhi Version Bump Script
# SemVer: MAJOR.MINOR.PATCH
# Usage: ./bump-version.sh <new_version>
# Example: ./bump-version.sh 0.8.0

set -e

if [ -z "$1" ]; then
    echo "Current version: $(cat VERSION | tr -d '[:space:]')"
    echo "Usage: $0 <new_version>"
    echo "Example: $0 0.8.0"
    exit 1
fi

NEW_VERSION="$1"
OLD_VERSION=$(cat VERSION | tr -d '[:space:]')

echo "Bumping version: $OLD_VERSION -> $NEW_VERSION"

# Update VERSION file
echo "$NEW_VERSION" > VERSION

# Update Cargo.toml
sed -i "s/^version = \"$OLD_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml

# Verify
CARGO_VER=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo ""
echo "Updated files:"
echo "  VERSION    -> $NEW_VERSION"
echo "  Cargo.toml -> $CARGO_VER"
echo ""
echo "Don't forget to update CHANGELOG.md!"
