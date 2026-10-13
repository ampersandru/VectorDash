param([switch]$SkipBuild)
$ErrorActionPreference='Stop'
& (Join-Path $PSScriptRoot 'Build-Rust.ps1') -SkipBuild:$SkipBuild
