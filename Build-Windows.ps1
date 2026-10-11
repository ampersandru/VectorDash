param([string]$Python = '', [switch]$SkipDependencies)
$ErrorActionPreference = 'Stop'
$ProjectRoot = $PSScriptRoot
Set-Location $ProjectRoot
if (!$Python) {
    if (!(Test-Path '.build-venv/Scripts/python.exe')) {
        py -3 -m venv .build-venv
        if ($LASTEXITCODE) { throw 'Unable to create build environment' }
    }
    $Python = Join-Path $ProjectRoot '.build-venv/Scripts/python.exe'
}
if (!$SkipDependencies) {
    & $Python -m pip install -r packaging/requirements-build.txt
    if ($LASTEXITCODE) { throw 'Dependency installation failed' }
}
$BuildPath = $env:PATH
try {
    # Do not collect unrelated DLLs from tools installed on the build machine.
    $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
    & $Python -m PyInstaller --noconfirm --clean packaging/VectorDash.spec
} finally {
    $env:PATH = $BuildPath
}
if ($LASTEXITCODE) { throw 'Application build failed' }
$Compiler = Join-Path $ProjectRoot '.build-tools/inno/ISCC.exe'
if (!(Test-Path $Compiler)) {
    New-Item -ItemType Directory -Force '.build-tools' | Out-Null
    $Download = Join-Path $ProjectRoot '.build-tools/innosetup-6.7.3.exe'
    Invoke-WebRequest 'https://github.com/jrsoftware/issrc/releases/download/is-6_7_3/innosetup-6.7.3.exe' -OutFile $Download
    if ((Get-FileHash $Download -Algorithm SHA256).Hash.ToLowerInvariant() -ne '9c73c3bae7ed48d44112a0f48e66742c00090bdb5bef71d9d3c056c66e97b732') { throw 'Compiler checksum mismatch' }
    $Install = Start-Process -FilePath $Download -ArgumentList @('/PORTABLE=1','/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/NOICONS','/TASKS=""',('/DIR="' + (Join-Path $ProjectRoot '.build-tools/inno') + '"')) -WindowStyle Hidden -Wait -PassThru
    if ($Install.ExitCode) { throw 'Compiler setup failed' }
}
& $Compiler packaging/VectorDash.iss
if ($LASTEXITCODE) { throw 'Installer build failed' }
$Installer = Join-Path $ProjectRoot 'dist/installer/VectorDash-Setup-0.2.14-x64.exe'
$Hash = (Get-FileHash $Installer -Algorithm SHA256).Hash.ToLowerInvariant()
"$Hash  $(Split-Path $Installer -Leaf)" | Set-Content 'dist/installer/SHA256SUMS.txt' -Encoding ascii
Write-Output "Installer ready: $Installer"
