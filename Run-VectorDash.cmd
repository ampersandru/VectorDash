@echo off
setlocal
if not exist "%~dp0dist\VectorDashRust\VectorDash.exe" (
 echo Build the native program first with Build-Rust.ps1.
 pause
 exit /b 1
)
start "" "%~dp0dist\VectorDashRust\VectorDash.exe"
