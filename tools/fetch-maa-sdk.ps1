#Requires -Version 5.1
<#
.SYNOPSIS
下载 MaaFramework 官方预编译运行时到 ./maa-sdk。

.DESCRIPTION
MaaWizard 采用 dynamic 链接模式，编译期不需要 SDK，但运行期必须通过
maa_framework::load_library 加载 MaaFramework.dll，因此首次运行前需要本脚本。

全程非交互：自动探测 Windows x86_64/aarch64，下载最新 release 并解压。
#>
[CmdletBinding()]
param(
    [string]$Version = "latest"
)

$ErrorActionPreference = "Stop"

# 依据处理器架构选择官方发布包前缀
$arch = if ([System.Environment]::Is64BitOperatingSystem) {
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
} else {
    "x86"
}
$assetPattern = "MAA-win-$arch-*.zip"

Write-Host "目标架构: win-$arch，匹配资源: $assetPattern"

# 查询最新 release 的资产列表
$apiUrl = if ($Version -eq "latest") {
    "https://api.github.com/repos/MaaXYZ/MaaFramework/releases/latest"
} else {
    "https://api.github.com/repos/MaaXYZ/MaaFramework/releases/tags/$Version"
}

$release = Invoke-RestMethod -Uri $apiUrl -Headers @{ "User-Agent" = "MaaWizard" }
$asset = $release.assets | Where-Object { $_.name -like $assetPattern } | Select-Object -First 1
if (-not $asset) {
    throw "未在 release $($release.tag_name) 中找到匹配 $assetPattern 的资源"
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$sdkDir = Join-Path $repoRoot "maa-sdk"
$zipPath = Join-Path $env:TEMP $asset.name

Write-Host "下载 $($asset.name) ($([math]::Round($asset.size / 1MB, 1)) MB) ..."
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath

Write-Host "解压到 $sdkDir ..."
if (Test-Path $sdkDir) { Remove-Item -Recurse -Force $sdkDir }
Expand-Archive -Path $zipPath -DestinationPath $sdkDir -Force
Remove-Item -Force $zipPath

# 官方包内通常还有一层以版本号命名的目录，将其内容提升到 maa-sdk 根目录
$inner = Get-ChildItem -Path $sdkDir -Directory | Select-Object -First 1
if ($inner -and (Test-Path (Join-Path $inner.FullName "bin"))) {
    Get-ChildItem -Path $inner.FullName | ForEach-Object {
        Move-Item -Path $_.FullName -Destination $sdkDir -Force
    }
    Remove-Item -Recurse -Force $inner.FullName
}

$dll = Get-ChildItem -Path $sdkDir -Filter "MaaFramework.dll" -Recurse | Select-Object -First 1
if ($dll) {
    Write-Host "完成：$($dll.FullName)"
    Write-Host "请在界面中填入 DLL 路径（相对 src-tauri 工作目录）：maa-sdk/bin/MaaFramework.dll"
} else {
    Write-Warning "已解压到 $sdkDir，但未找到 MaaFramework.dll，请检查包结构"
}
