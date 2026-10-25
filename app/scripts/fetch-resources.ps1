# Fills app/resources with everything the self-contained installer ships:
#   postgresql/  PostgreSQL server binaries (bin, lib, share)
#   llama/       llama.cpp llama-server for the advisor (CPU build)
#   models/      the advisor model, advisor.gguf
#   web/         the built web interface (run `npm run build` in web/ first)
# Every download is pinned and checked against its SHA-256.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # the progress bar makes Invoke-WebRequest many times slower

$PG_VERSION    = '18.6.1'
$PG_SHA256     = 'd82142f3a9a49be8467f0e87f5a079f627515539f94a070fe1463208fa8eb3c6'
$LLAMA_TAG     = 'b11552'
$LLAMA_SHA256  = 'c00741179811ff903b515eae28bdeef1f47af6e81c49920c4e68b62954e056d6'
$MODEL_URL     = 'https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/main/Qwen3.5-2B-Q4_K_M.gguf'
$MODEL_SHA256  = 'aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223'

$app  = Split-Path -Parent $PSScriptRoot
$res  = Join-Path $app 'resources'
$dl   = Join-Path $app '.downloads'
$repo = Split-Path -Parent $app
New-Item -ItemType Directory -Force -Path $res, $dl | Out-Null

function Get-Checked([string]$url, [string]$sha, [string]$name) {
    $file = Join-Path $dl $name
    if ((Test-Path $file) -and ((Get-FileHash $file -Algorithm SHA256).Hash -eq $sha.ToUpper())) {
        Write-Host "cached and verified $name"
        return $file
    }
    Write-Host "downloading (resumable) $url"
    for ($i = 1; $i -le 5; $i++) {
        try {
            & curl.exe -L -C - --retry 5 --retry-delay 3 --silent --show-error -o $file $url
            if ($LASTEXITCODE -eq 0) { break }
        }
        catch { 
            if ($i -eq 5) { throw }
            Start-Sleep -Seconds (5 * $i) 
        }
    }
    $got = (Get-FileHash $file -Algorithm SHA256).Hash
    if ($got -ne $sha.ToUpper()) { 
        Remove-Item -Force $file -ErrorAction SilentlyContinue
        throw "$name checksum mismatch: got $got, expected $sha" 
    }
    return $file
}

function Expand-Into([string]$zip, [string]$dest, [string]$marker) {
    $tmp = Join-Path $dl ([IO.Path]::GetFileNameWithoutExtension($zip))
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
    Expand-Archive -Path $zip -DestinationPath $tmp
    # Archives differ in whether they wrap everything in a top folder; find the real root.
    $hit = Get-ChildItem -Path $tmp -Recurse -Filter $marker | Select-Object -First 1
    if (-not $hit) { throw "$marker not found in $zip" }
    return $hit.Directory
}

# The Visual C++ runtime is not on every Windows install; Microsoft allows shipping these
# DLLs next to the programs that need them (app-local deployment).
function Copy-VcRuntime([string]$dir) {
    foreach ($dll in 'vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll') {
        $src = Join-Path $env:WINDIR "System32\$dll"
        if ((Test-Path $src) -and -not (Test-Path (Join-Path $dir $dll))) { Copy-Item $src $dir }
    }
}

# ---- PostgreSQL ---------------------------------------------------------------------
$pgZip = Get-Checked "https://github.com/theseus-rs/postgresql-binaries/releases/download/$PG_VERSION/postgresql-$PG_VERSION-x86_64-pc-windows-msvc.zip" $PG_SHA256 "postgresql-$PG_VERSION.zip"
$pgBin = Expand-Into $pgZip "$res\postgresql" 'pg_ctl.exe'
$pgRoot = $pgBin.Parent.FullName
$pgOut = Join-Path $res 'postgresql'
if (Test-Path $pgOut) { Remove-Item -Recurse -Force $pgOut }
New-Item -ItemType Directory -Force -Path $pgOut | Out-Null
foreach ($part in 'bin', 'lib', 'share') {
    if (Test-Path (Join-Path $pgRoot $part)) { Copy-Item -Recurse (Join-Path $pgRoot $part) $pgOut }
}
# Headers, static libraries and debug symbols are not needed to run the server.
Get-ChildItem -Path $pgOut -Recurse -Include *.pdb, *.lib, *.a | Remove-Item -Force
Copy-VcRuntime (Join-Path $pgOut 'bin')
if (-not (Test-Path "$pgOut\bin\initdb.exe")) { throw 'initdb.exe missing' }

# ---- llama.cpp server ---------------------------------------------------------------
$llZip = Get-Checked "https://github.com/ggml-org/llama.cpp/releases/download/$LLAMA_TAG/llama-$LLAMA_TAG-bin-win-cpu-x64.zip" $LLAMA_SHA256 "llama-$LLAMA_TAG.zip"
$llDir = Expand-Into $llZip "$res\llama" 'llama-server.exe'
$llOut = Join-Path $res 'llama'
if (Test-Path $llOut) { Remove-Item -Recurse -Force $llOut }
New-Item -ItemType Directory -Force -Path $llOut | Out-Null
# The server and the libraries it loads; the other command-line tools are left out.
Copy-Item (Join-Path $llDir.FullName 'llama-server.exe') $llOut
Get-ChildItem -Path $llDir.FullName -Filter *.dll | Copy-Item -Destination $llOut
Copy-VcRuntime $llOut

# ---- model --------------------------------------------------------------------------
$model = Get-Checked $MODEL_URL $MODEL_SHA256 'advisor.gguf'
New-Item -ItemType Directory -Force -Path (Join-Path $res 'models') | Out-Null
Copy-Item $model (Join-Path $res 'models\advisor.gguf') -Force

# ---- web interface ------------------------------------------------------------------
$dist = Join-Path $repo 'web\dist'
if (-not (Test-Path "$dist\index.html")) { throw 'web/dist is missing: run npm run build in web/ first' }
$webOut = Join-Path $res 'web'
if (Test-Path $webOut) { Remove-Item -Recurse -Force $webOut }
Copy-Item -Recurse $dist $webOut

$size = (Get-ChildItem -Recurse $res | Measure-Object -Property Length -Sum).Sum / 1MB
Write-Host ("resources ready: {0:N0} MB" -f $size)
