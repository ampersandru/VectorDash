@echo off
cd /d "%~dp0"
if not exist ".venv\Scripts\pythonw.exe" (
  echo Run Setup-VectorDash.cmd once to install the desktop dependencies.
  pause
  exit /b 1
)
.venv\Scripts\python.exe -c "import numpy, PySide6.QtWidgets, PySide6.QtMultimedia, psutil, PIL, pymupdf, imageio_ffmpeg" >nul 2>&1
if errorlevel 1 (
  echo Desktop dependencies are missing. Run Setup-VectorDash.cmd.
  pause
  exit /b 1
)
start "" ".venv\Scripts\pythonw.exe" "%~dp0desktop.py"
