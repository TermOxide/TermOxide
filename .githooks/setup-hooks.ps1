# Setup script for configuring Git hooks path
# Usage: (root of the repo)/.githooks/setup-hooks.ps1

$HooksDir = ".githooks"

# Verify we are in the root of the git repository
if (!(Test-Path -Path ".git")) {
    Write-Host "Error: This script must be run from the root of the repository." -ForegroundColor Red
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
