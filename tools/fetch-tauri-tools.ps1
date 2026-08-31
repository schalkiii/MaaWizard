#Requires -Version 5.1
<#
.SYNOPSIS
Pre-download the Tauri NSIS 3.11 Windows bundling tool into the local cache
dir (%LOCALAPPDATA%\tauri\NSIS) so that `tauri build` (nsis target) skips the
online download. WiX is no longer used (bundle.targets = ["nsis"]).

Uses curl with resume (-C -) + auto-retry. Defaults to ghfast.top mirror,
falls back to direct GitHub on failure.
#>
$ErrorActionPreference = "Stop"
$Mirror = "https://ghfast.top/"

$base = Join-Path $env:LOCALAPPDATA 'tauri'
$tmp  = Join-Path $env:TEMP 'tauri-tools'
New-Item -ItemType Directory -Force -Path $base, $tmp | Out-Null

function Get-File($label, $url, $outPath) {
    if (Test-Path $outPath) { Write-Host "[$label] resume from $((Get-Item $outPath).Length) bytes" }
    $murl = "$Mirror$url"
    for ($i = 1; $i -le 200; $i++) {
        & curl.exe -L -C - --retry 1 --max-time 600 -o $outPath $murl
        if ($LASTEXITCODE -eq 0) { Write-Host "[$label] done $((Get-Item $outPath).Length) bytes"; return $true }
        Write-Host "[$label] attempt $i interrupted (exit=$LASTEXITCODE), retry in 3s..."
        Start-Sleep -Seconds 3
    }
    Write-Warning "[$label] mirror failed, fallback to direct"
    for ($i = 1; $i -le 200; $i++) {
        & curl.exe -L -C - --retry 1 --max-time 600 -o $outPath $url
        if ($LASTEXITCODE -eq 0) { Write-Host "[$label] direct done $((Get-Item $outPath).Length) bytes"; return $true }
        Start-Sleep -Seconds 3
    }
    return $false
}

# --- NSIS 3.11 ---
$nsisUrl = 'https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.11/nsis-3.11.zip'
$nsisZip = Join-Path $tmp 'nsis.zip'
$nsisDir = Join-Path $base 'NSIS'
if (-not (Test-Path $nsisDir)) { New-Item -ItemType Directory -Force -Path $nsisDir | Out-Null }
if (-not (Get-File 'NSIS' $nsisUrl $nsisZip)) { throw 'NSIS download failed' }
Expand-Archive -Path $nsisZip -DestinationPath $nsisDir -Force

# --- nsis_tauri_utils.dll ---
$dllUrl = 'https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.5.3/nsis_tauri_utils.dll'
$dllTmp = Join-Path $tmp 'nsis_tauri_utils.dll'
if (-not (Get-File 'NSIS-util' $dllUrl $dllTmp)) { throw 'nsis_tauri_utils.dll download failed' }
$dst = Join-Path $nsisDir 'plugins\x86-unicode\additional\nsis_tauri_utils.dll'
New-Item -ItemType Directory -Force -Path (Split-Path $dst) | Out-Null
Copy-Item $dllTmp $dst -Force
Write-Host "nsis_tauri_utils.dll -> $dst : $(Test-Path $dst)"

Write-Host 'TAURI TOOLS DONE'
