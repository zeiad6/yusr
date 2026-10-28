# Script triggered on-demand by the installer or user to fetch the advisor model
# Writes directly to %LOCALAPPDATA%\Yusr\models\advisor.gguf with SHA-256 validation.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$MODEL_URL = 'https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/main/Qwen3.5-2B-Q4_K_M.gguf'
$MODEL_SHA256 = 'aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223'

$localAppData = [System.Environment]::GetFolderPath([System.Environment+SpecialFolder]::LocalApplicationData)
if (-not $localAppData) { $localAppData = "$env:USERPROFILE\AppData\Local" }
$targetDir = Join-Path $localAppData "Yusr\models"
$targetFile = Join-Path $targetDir "advisor.gguf"
$partFile = "$targetFile.part"

New-Item -ItemType Directory -Force -Path $targetDir | Out-Null

if ((Test-Path $targetFile) -and ((Get-FileHash $targetFile -Algorithm SHA256).Hash -eq $MODEL_SHA256.ToUpper())) {
    exit 0
}

Write-Host "Downloading Yusr AI Advisor Model (~1.3 GB)..."
& curl.exe -L -C - --retry 8 --retry-delay 3 --silent --show-error -o $partFile $MODEL_URL
if ($LASTEXITCODE -ne 0) {
    throw "Curl download failed with code $LASTEXITCODE"
}

$hash = (Get-FileHash $partFile -Algorithm SHA256).Hash
if ($hash -ne $MODEL_SHA256.ToUpper()) {
    Remove-Item -Force $partFile -ErrorAction SilentlyContinue
    throw "Checksum mismatch: expected $MODEL_SHA256, got $hash"
}

Move-Item -Path $partFile -Destination $targetFile -Force
Write-Host "Yusr AI Advisor Model downloaded successfully."
