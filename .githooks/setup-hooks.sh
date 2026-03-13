#!/bin/sh
# Setup script for configuring Git hooks path
# Usage: (root of the repo)/.githooks/setup-hooks.sh

HOOKS_DIR=".githooks"

# Verify we are in the root of the git repository
if [ ! -d ".git" ]; then
    echo "Error: This script must be run from the root of the repository."
    exit 1
fi

# Configure git to use the custom hooks directory
git config core.hooksPath "$HOOKS_DIR"

if [ $? -eq 0 ]; then
    echo "Git hooks configured successfully to use $HOOKS_DIR"
else
    echo "Error: Failed to set core.hooksPath"
    exit 1
fi
