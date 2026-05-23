# Get the latest version from the version file
#!/bin/bash
RELEASE_VERSION=$1
if [ -z "$RELEASE_VERSION" ]; then
    echo "Usage: $0 <release-version>"
    exit 1
fi
git tag -a "$RELEASE_VERSION" -m "Release $RELEASE_VERSION"
git push origin "$RELEASE_VERSION"
git push --tags
echo "Release $RELEASE_VERSION created and pushed successfully."