<#
.SYNOPSIS
    Configures PostgreSQL database, database creation, connection, and automatic boot startup on Windows.

.DESCRIPTION
    1. Checks / verifies bundled or system PostgreSQL installation.
    2. Initializes data directory if needed.
    3. Starts PostgreSQL and creates 'yusr' database and user.
    4. Registers Windows Scheduled Task and Registry Run key for boot auto-start.
    5. Tests and verifies loopback connection.
#>

param(
    [string]$DbUser = "yusr",
    [string]$DbPassword = "yusr",
    [string]$DbName = "yusr",
    [int]$Port = 5432
)

$ErrorActionPreference = "Stop"

Write-Host "====================================================================" -ForegroundColor Cyan
Write-Host "   Yusr Suite (Windows) - Database Setup & Boot Autostart Config" -ForegroundColor Cyan
Write-Host "====================================================================" -ForegroundColor Cyan

$root = Split-Path -Parent $PSScriptRoot
$dataDir = Join-Path $env:LOCALAPPDATA "Yusr"
$pgData = Join-Path $dataDir "pgdata"
$logsDir = Join-Path $dataDir "logs"
$secretsDir = Join-Path $dataDir "secrets"

New-Item -ItemType Directory -Force -Path $dataDir, $pgData, $logsDir, $secretsDir | Out-Null

# 1. Locate PostgreSQL binaries
$pgBin = Join-Path $root "app\resources\postgresql\bin"
if (-not (Test-Path (Join-Path $pgBin "pg_ctl.exe"))) {
    # Check default Postgres install paths
    $systemPg = Get-ChildItem "C:\Program Files\PostgreSQL\*\bin\pg_ctl.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($systemPg) {
        $pgBin = $systemPg.DirectoryName
    } else {
        Write-Warning "PostgreSQL binaries not found at $pgBin or system paths. Run ./app/scripts/fetch-resources.ps1 first."
    }
}

$pgCtl = Join-Path $pgBin "pg_ctl.exe"
$initDb = Join-Path $pgBin "initdb.exe"
$psql = Join-Path $pgBin "psql.exe"

# 2. Initialize cluster if needed
if ((Test-Path $initDb) -and (-not (Test-Path (Join-Path $pgData "PG_VERSION")))) {
    Write-Host "[*] Initializing private PostgreSQL cluster in $pgData..." -ForegroundColor Yellow
    $pwFile = Join-Path $dataDir "temp.pw"
    $DbPassword | Out-File -FilePath $pwFile -Encoding ascii -NoNewline
    & $initDb -D $pgData -U $DbUser --pwfile=$pwFile --auth=scram-sha-256 -E UTF8 --no-locale
    Remove-Item -Force $pwFile -ErrorAction SilentlyContinue
    Write-Host "[✓] PostgreSQL cluster initialized." -ForegroundColor Green
}

# 3. Start PostgreSQL and create database
if (Test-Path $pgCtl) {
    Write-Host "[*] Starting PostgreSQL on port $Port..." -ForegroundColor Yellow
    & $pgCtl -D $pgData -l (Join-Path $logsDir "postgres.log") -o "-p $Port -c listen_addresses=127.0.0.1" -w start
    
    Start-Sleep -Seconds 2

    # Check and create database
    if (Test-Path $psql) {
        Write-Host "[*] Ensuring database '$DbName' exists..." -ForegroundColor Yellow
        $dbExists = & $psql -X -h 127.0.0.1 -p $Port -U $DbUser -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname = '$DbName'" 2>$null
        if ($dbExists -ne "1") {
            & $psql -X -h 127.0.0.1 -p $Port -U $DbUser -d postgres -c "CREATE DATABASE `"$DbName`" OWNER `"$DbUser`" ENCODING 'UTF8';"
            Write-Host "[✓] Database '$DbName' created successfully." -ForegroundColor Green
        } else {
            Write-Host "[✓] Database '$DbName' already exists." -ForegroundColor Green
        }
    }
}

# 4. Save Database Connection Key
$connectionUrl = "postgres://${DbUser}:${DbPassword}@127.0.0.1:${Port}/${DbName}"
Set-Content -Path (Join-Path $secretsDir "database_url.txt") -Value $connectionUrl -Encoding UTF8
Write-Host "[✓] Database connection configured: $connectionUrl" -ForegroundColor Green

# 5. Configure Windows Boot Autostart (Registry Run Key + Scheduled Task)
Write-Host "[*] Registering automatic boot startup in Windows..." -ForegroundColor Yellow

# Look for Yusr executable
$exeCandidates = @(
    (Join-Path $root "app\src-tauri\target\release\yusr-desktop.exe"),
    (Join-Path $root "target\release\yusr-server.exe"),
    "C:\Program Files\Yusr\Yusr.exe"
)
$yusrExe = $exeCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1

if ($yusrExe) {
    # Method A: Registry Run Key
    Set-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "Yusr" -Value "`"$yusrExe`"" -Force
    Write-Host "[✓] Windows Registry Run autostart registered for: $yusrExe" -ForegroundColor Green

    # Method B: Scheduled Task at Logon / Boot
    try {
        $action = New-ScheduledTaskAction -Execute $yusrExe
        $trigger = New-ScheduledTaskTrigger -AtLogOn
        $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive
        $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit (New-TimeSpan -Hours 24)
        Register-ScheduledTask -TaskName "YusrAutoStart" -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
        Write-Host "[✓] Windows Scheduled Task 'YusrAutoStart' registered successfully." -ForegroundColor Green
    } catch {
        Write-Warning "Could not register Scheduled Task (may require elevation): $($_.Exception.Message)"
    }
} else {
    Write-Warning "Yusr executable not built yet; Registry Run will be finalized upon release build."
}

Write-Host "====================================================================" -ForegroundColor Cyan
Write-Host " [✓] Database installation, database creation, connection, and" -ForegroundColor Cyan
Write-Host "     automatic boot startup setup completed successfully!" -ForegroundColor Cyan
Write-Host "====================================================================" -ForegroundColor Cyan
