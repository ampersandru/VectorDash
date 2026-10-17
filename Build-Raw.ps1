# Pinned optional camera decoder. Runs only when Sources > camera RAW is enabled.
$ErrorActionPreference='Stop'
$taskRawRoot=Join-Path $PSScriptRoot '.build-tools/raw'
$taskRawZip=Join-Path $taskRawRoot 'LibRaw-0.22.2-Win64.zip'
New-Item -ItemType Directory -Force $taskRawRoot | Out-Null
if (!(Test-Path -LiteralPath $taskRawZip)) {
 Invoke-WebRequest 'https://www.libraw.org/data/LibRaw-0.22.2-Win64.zip' -OutFile $taskRawZip
}
if ((Get-FileHash -LiteralPath $taskRawZip -Algorithm SHA256).Hash.ToLowerInvariant() -ne 'ac64fa12bb00a7581332d4c6ab918c0533fb3f119d6b668d47a6875410dca948') {throw 'LibRaw checksum mismatch'}
Expand-Archive -LiteralPath $taskRawZip -DestinationPath $taskRawRoot -Force
$taskRawSource=Join-Path $taskRawRoot 'LibRaw-0.22.2'
$taskVswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$taskVs=& $taskVswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
$taskUseDevShell=$false
if (!$taskVs -and $env:VSINSTALLDIR -and $env:VSCMD_ARG_TGT_ARCH -eq 'x64') {
 $taskCl=Get-Command cl.exe -ErrorAction SilentlyContinue
 $taskCandidate=[IO.Path]::GetFullPath($env:VSINSTALLDIR)
 if ($taskCl -and $taskCl.Source.StartsWith((Join-Path $taskCandidate 'VC'),[StringComparison]::OrdinalIgnoreCase)) {
  $taskVs=$taskCandidate
  $taskUseDevShell=$true
 }
}
if (!$taskVs) {throw 'MSVC C++ build tools required to build the isolated LibRaw adapter'}
$taskVcvars=Join-Path $taskVs 'VC/Auxiliary/Build/vcvars64.bat'
$taskRawOut=Join-Path $PSScriptRoot 'native/target/raw-decoder'
New-Item -ItemType Directory -Force $taskRawOut | Out-Null
$taskCpp=Join-Path $PSScriptRoot 'native/raw/decode.cpp'
# Compilation only: no destructive filesystem operations are delegated to cmd.
$taskCompile='cl /nologo /std:c++17 /EHsc /O2 /MD /I"'+$taskRawSource+'" "'+$taskCpp+'" /Fo"'+$taskRawOut+'/decode.obj" /Fe"'+$taskRawOut+'/vectordash-raw.exe" /link "'+$taskRawSource+'/lib/libraw.lib"'
$taskBat=Join-Path $taskRawOut 'compile.cmd'
$taskCommands=@('@echo off')
if (!$taskUseDevShell) {$taskCommands+=('call "'+$taskVcvars+'" >nul');$taskCommands+='if errorlevel 1 exit /b 1'}
$taskCommands+=@($taskCompile,'exit /b %errorlevel%')
$taskCommands | Set-Content -LiteralPath $taskBat -Encoding ascii
& $env:ComSpec /d /c $taskBat
if ($LASTEXITCODE) {throw 'RAW adapter build failed'}
Copy-Item -LiteralPath (Join-Path $taskRawSource 'bin/libraw.dll') -Destination $taskRawOut -Force
# App-local CRT for machines without the Visual C++ redistributable installed.
$taskRedist=Get-ChildItem -LiteralPath (Join-Path $taskVs 'VC/Redist/MSVC') -Directory | Where-Object {$_.Name -match '^\d'} | Sort-Object Name -Descending | Select-Object -First 1
$taskCrt=Join-Path $taskRedist.FullName 'x64/Microsoft.VC143.CRT'
if (!(Test-Path -LiteralPath $taskCrt)) {$taskCrt=Join-Path $taskRedist.FullName 'x64/Microsoft.VC145.CRT'}
if (!(Test-Path -LiteralPath $taskCrt)) {throw 'MSVC redistributable CRT not found'}
Get-ChildItem -LiteralPath $taskCrt -File -Filter '*.dll' | ForEach-Object {Copy-Item -LiteralPath $_.FullName -Destination $taskRawOut -Force}
