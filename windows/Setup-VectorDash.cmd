@echo off
cd /d "%~dp0"
if not exist ".venv\Scripts\python.exe" (
  python -m venv .venv
  if errorlevel 1 goto failed
)
.venv\Scripts\python.exe -m pip install -r requirements.txt
if errorlevel 1 goto failed
echo Setup complete. Open Run-VectorDash.cmd to launch the app.
pause
exit /b 0
:failed
echo Setup failed. Install Python 3.12 or later and try again.
pause
exit /b 1
