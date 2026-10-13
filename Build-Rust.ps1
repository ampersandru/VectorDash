param([switch]$SkipBuild)
$ErrorActionPreference='Stop'
Set-Location $PSScriptRoot
if (!(Test-Path '.build-tools/perl/perl/bin/perl.exe')) {
 New-Item -ItemType Directory -Force '.build-tools/perl' | Out-Null
 $taskPerlZip=Join-Path $PSScriptRoot '.build-tools/perl/perl.zip'
 Invoke-WebRequest 'https://github.com/StrawberryPerl/Perl-Dist-Strawberry/releases/download/SP_54231_64bit/strawberry-perl-5.42.3.1-64bit-portable.zip' -OutFile $taskPerlZip
 if ((Get-FileHash $taskPerlZip -Algorithm SHA256).Hash.ToLowerInvariant() -ne '6a081a811781c30aca51dbc036afd93092af91e3297901f02c17043795a10690') {throw 'Perl build-tool checksum mismatch'}
 Expand-Archive -LiteralPath $taskPerlZip -DestinationPath '.build-tools/perl' -Force
}
$taskOriginalPath=$env:PATH
try {
 $env:PATH=(Join-Path $PSScriptRoot '.build-tools/perl/perl/bin')+';'+(Join-Path $PSScriptRoot '.build-tools/perl/c/bin')+';'+$env:PATH
 if (!$SkipBuild) {cargo build --locked --release --manifest-path native/Cargo.toml; if ($LASTEXITCODE) {throw 'Rust build failed'}}
} finally {$env:PATH=$taskOriginalPath}
$taskBundle=Join-Path $PSScriptRoot 'dist/VectorDashRust'
if (Test-Path $taskBundle) { throw 'Bundle directory already exists. Use a fresh staging directory; never package a previously run portable app.' }
New-Item -ItemType Directory -Force $taskBundle | Out-Null
Copy-Item -LiteralPath 'native/target/release/vectordash.exe' -Destination (Join-Path $taskBundle 'VectorDash.exe')
if (!(Test-Path '.build-tools/pdfium/bin/pdfium.dll')) {
 New-Item -ItemType Directory -Force '.build-tools/pdfium' | Out-Null
 $taskPdfArchive=Join-Path $PSScriptRoot '.build-tools/pdfium/pdfium-win-x64.tgz'
 Invoke-WebRequest 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8086/pdfium-win-x64.tgz' -OutFile $taskPdfArchive
 if ((Get-FileHash $taskPdfArchive -Algorithm SHA256).Hash.ToLowerInvariant() -ne '1fd8af952832dbb0eb16d9249f68fe09e5f5ebf7c3dd9f6066ea2720cc28487d') {throw 'PDFium checksum mismatch'}
 tar -xzf $taskPdfArchive -C '.build-tools/pdfium'
 if ($LASTEXITCODE) {throw 'PDFium extraction failed'}
}
Copy-Item -LiteralPath '.build-tools/pdfium/bin/pdfium.dll' -Destination $taskBundle
# Native decoder/player only; no Python package is needed to build or run.
if (!(Test-Path '.build-tools/ffmpeg-rust/ffmpeg.exe')) {
 New-Item -ItemType Directory -Force '.build-tools/ffmpeg-rust' | Out-Null
 $taskFfmpegZip=Join-Path $PSScriptRoot '.build-tools/ffmpeg-rust/ffmpeg-essentials.zip'
 Invoke-WebRequest 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip' -OutFile $taskFfmpegZip
 if ((Get-FileHash $taskFfmpegZip -Algorithm SHA256).Hash.ToLowerInvariant() -ne '60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba') {throw 'FFmpeg publisher checksum mismatch; update the pinned release explicitly'}
 Expand-Archive -LiteralPath $taskFfmpegZip -DestinationPath '.build-tools/ffmpeg-rust/extracted' -Force
 foreach ($taskName in @('ffmpeg.exe','ffplay.exe','ffprobe.exe')) {$taskFile=Get-ChildItem '.build-tools/ffmpeg-rust/extracted' -Recurse -Filter $taskName | Select-Object -First 1;Copy-Item -LiteralPath $taskFile.FullName -Destination (Join-Path $PSScriptRoot '.build-tools/ffmpeg-rust')}
}
if (!(Test-Path '.build-tools/ffmpeg-rust/LICENSE')) {
 $taskFfmpegLicense=Get-ChildItem '.build-tools/ffmpeg-rust/extracted' -Recurse -File -Filter LICENSE | Select-Object -First 1
 if (!$taskFfmpegLicense) {throw 'Verified FFmpeg archive lacks its full license'}
 Copy-Item -LiteralPath $taskFfmpegLicense.FullName -Destination '.build-tools/ffmpeg-rust/LICENSE'
}
foreach ($taskName in @('ffmpeg.exe','ffplay.exe','ffprobe.exe')) {Copy-Item -LiteralPath (Join-Path $PSScriptRoot ('.build-tools/ffmpeg-rust/'+$taskName)) -Destination $taskBundle -Force}
Copy-Item -LiteralPath LICENSE -Destination $taskBundle
$taskNotices=Join-Path $taskBundle 'licenses'
New-Item -ItemType Directory -Force $taskNotices | Out-Null
Copy-Item -LiteralPath 'packaging/notices/SQLCipher-LICENSE.txt','packaging/notices/OpenSSL-LICENSE.txt' -Destination $taskNotices
Copy-Item -LiteralPath 'native/assets/mdi/LICENSE.txt' -Destination (Join-Path $taskNotices 'MDI-LICENSE.txt')
Copy-Item -LiteralPath '.build-tools/pdfium/LICENSE' -Destination (Join-Path $taskNotices 'PDFium-LICENSE.txt')
Copy-Item -LiteralPath '.build-tools/pdfium/licenses' -Destination (Join-Path $taskNotices 'PDFium') -Recurse -Force
Copy-Item -LiteralPath '.build-tools/ffmpeg-rust/LICENSE' -Destination (Join-Path $taskNotices 'FFmpeg-COPYING.txt')
& (Join-Path $taskBundle 'ffmpeg.exe') -L *> (Join-Path $taskNotices 'FFmpeg-LICENSE.txt')
$taskCargoCache=Join-Path $env:USERPROFILE '.cargo/registry/src/index.crates.io-1949cf8c6b5b557f'
$taskLock=Get-Content 'native/Cargo.lock' -Raw
foreach ($taskPackage in [regex]::Matches($taskLock,'(?ms)\[\[package\]\]\s+name = "([^"]+)"\s+version = "([^"]+)"')) {
 $taskCrate=$taskPackage.Groups[1].Value+'-'+$taskPackage.Groups[2].Value
 $taskSource=Join-Path $taskCargoCache $taskCrate
 if (!(Test-Path $taskSource)) {continue}
 $taskFiles=Get-ChildItem -LiteralPath $taskSource -File | Where-Object {$_.Name -match '^(LICENSE|COPYING|NOTICE|UNLICENSE)'}
 if ($taskFiles) {$taskDest=Join-Path $taskNotices $taskCrate;New-Item -ItemType Directory -Force $taskDest | Out-Null;foreach($taskFile in $taskFiles){Copy-Item -LiteralPath $taskFile.FullName -Destination $taskDest}}
}
$taskCompiler=Join-Path $PSScriptRoot '.build-tools/inno/ISCC.exe'
if (!(Test-Path $taskCompiler)) {
 New-Item -ItemType Directory -Force '.build-tools' | Out-Null
 $taskInno=Join-Path $PSScriptRoot '.build-tools/innosetup-6.7.3.exe'
 Invoke-WebRequest 'https://github.com/jrsoftware/issrc/releases/download/is-6_7_3/innosetup-6.7.3.exe' -OutFile $taskInno
 if ((Get-FileHash $taskInno -Algorithm SHA256).Hash.ToLowerInvariant() -ne '9c73c3bae7ed48d44112a0f48e66742c00090bdb5bef71d9d3c056c66e97b732') {throw 'Installer compiler checksum mismatch'}
 $taskInstall=Start-Process -FilePath $taskInno -ArgumentList @('/PORTABLE=1','/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/NOICONS','/TASKS=""',('/DIR="'+(Join-Path $PSScriptRoot '.build-tools/inno')+'"')) -WindowStyle Hidden -Wait -PassThru
 if ($taskInstall.ExitCode) {throw 'Installer compiler bootstrap failed'}
}
& $taskCompiler packaging/VectorDash-Rust.iss
if ($LASTEXITCODE) {throw 'Native installer build failed'}
$taskPortable=Join-Path $PSScriptRoot 'dist/VectorDashRustPortable'
if (Test-Path $taskPortable) { throw 'Portable staging directory already exists. Refusing to package potential user data.' }
New-Item -ItemType Directory -Force $taskPortable | Out-Null
Get-ChildItem -LiteralPath $taskBundle | Copy-Item -Destination $taskPortable -Recurse -Force
Set-Content -LiteralPath (Join-Path $taskPortable 'portable.flag') -Value '' -Encoding ascii
Compress-Archive -Path (Join-Path $taskPortable '*') -DestinationPath 'dist/installer/VectorDash-Rust-0.3.6-x64.zip' -Force
$taskInstaller='dist/installer/VectorDash-Rust-Setup-0.3.6-x64.exe'
$taskZip='dist/installer/VectorDash-Rust-0.3.6-x64.zip'
@($taskInstaller,$taskZip) | ForEach-Object {((Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()+'  '+(Split-Path $_ -Leaf))} | Set-Content 'dist/installer/RUST-SHA256SUMS.txt' -Encoding ascii
Write-Output "Native installer and portable ZIP ready."
