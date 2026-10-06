# Setup script for configuring Git hooks path
# Usage: (root of the repo)/.githooks/setup-hooks.ps1

$HooksDir = ".githooks"

# Verify we are inside a git repository
git rev-parse --git-dir 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: This script must be run from inside a git repository." -ForegroundColor Red
    exit 1
}

# Configure git to use the custom hooks directory
git config core.hooksPath $HooksDir
if ($LASTEXITCODE -eq 0) {
    Write-Host "Git hooks configured successfully to use $HooksDir" -ForegroundColor Green
} else {
    Write-Host "Error: Failed to set core.hooksPath" -ForegroundColor Red
    exit 1
}
