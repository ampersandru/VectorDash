"""Windows integrations; no administrator access required."""
import ctypes
from ctypes import wintypes
import os
from pathlib import Path
import re
import subprocess
import process_lifecycle
import sys

HOTKEY_ID = 0xD160

def hotkey_parts(text):
    modifiers = {'ctrl': 2, 'alt': 1, 'shift': 4, 'win': 8}
    keys = {'space': 0x20, 'enter': 0x0D, 'tab': 0x09}
    parts = [part.strip().lower() for part in text.split('+')]
    if len(parts) < 2 or any(p not in modifiers for p in parts[:-1]):
        raise ValueError('Use a modifier and key, such as Ctrl+Shift+Space.')
    mask = 0x4000  # MOD_NOREPEAT
    for part in parts[:-1]:
        mask |= modifiers[part]
    key = parts[-1]
    if key in keys:
        code = keys[key]
    elif len(key) == 1 and key.isascii() and key.isalnum():
        code = ord(key.upper())
    elif re.fullmatch(r'f([1-9]|1[0-9]|2[0-4])', key):
        code = 0x70 + int(key[1:]) - 1
    else:
        raise ValueError('Supported keys: Space, Enter, Tab, letters, digits, F1â€“F24.')
    return mask, code

class GlobalHotkey:
    def __init__(self):
        self.current = ''
    def register(self, text):
        mask, key = hotkey_parts(text)
        if os.name != 'nt':
            raise OSError('Global shortcuts require Windows.')
        old = self.current
        self.unregister()
        if not ctypes.windll.user32.RegisterHotKey(None, HOTKEY_ID, mask, key):
            if old:
                previous = hotkey_parts(old)
                ctypes.windll.user32.RegisterHotKey(None, HOTKEY_ID, *previous)
                self.current = old
            raise OSError('This shortcut is already in use. Choose another combination.')
        self.current = text
    def unregister(self):
        if os.name == 'nt' and self.current:
            ctypes.windll.user32.UnregisterHotKey(None, HOTKEY_ID)
        self.current = ''


def set_startup(enabled):
    if os.name != 'nt':
        return
    import winreg
    key_path = r'Software\Microsoft\Windows\CurrentVersion\Run'
    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, key_path) as key:
        if enabled:
            executable = Path(sys.executable)
            if executable.name.lower() == 'python.exe':
                executable = executable.with_name('pythonw.exe')
            command = subprocess.list2cmdline([str(executable), str(Path(__file__).with_name('desktop.py').resolve()), '--startup'])
            winreg.SetValueEx(key, 'VectorDash', 0, winreg.REG_SZ, command)
            try:
                winreg.DeleteValue(key,'DigUp')
            except FileNotFoundError:
                pass
        else:
            try:
                winreg.DeleteValue(key, 'VectorDash')
            except FileNotFoundError:
                pass
            try:
                winreg.DeleteValue(key,'DigUp')
            except FileNotFoundError:
                pass


def idle_seconds():
    if os.name != 'nt':
        return None
    class LASTINPUTINFO(ctypes.Structure):
        _fields_ = [('cbSize', wintypes.UINT), ('dwTime', wintypes.DWORD)]
    info = LASTINPUTINFO()
    info.cbSize = ctypes.sizeof(info)
    if not ctypes.windll.user32.GetLastInputInfo(ctypes.byref(info)):
        return None
    return ((ctypes.windll.kernel32.GetTickCount() & 0xffffffff) - info.dwTime & 0xffffffff)/1000


def parse_devices(output):
    devices = []
    for line in output.splitlines():
        match = re.match(r'^\s*((?:CUDA|Vulkan|SYCL|ROCm|HIP|OpenCL|CPU)\d*)\s*:\s*(.+)', line, re.I)
        if match:
            devices.append((match[1], match[2].strip()))
    return list(dict.fromkeys(devices))


def gpu_utilization():
    """Conservative system-wide GPU engine utilization, not vendor-specific."""
    if os.name != 'nt':
        return None
    script = r"$ErrorActionPreference='Stop'; $v=(Get-Counter '\GPU Engine(*)\Utilization Percentage').CounterSamples | Where-Object CookedValue -ge 0 | Measure-Object CookedValue -Sum; if ($null -ne $v.Sum) { [Console]::WriteLine($v.Sum.ToString([Globalization.CultureInfo]::InvariantCulture)) }"
    try:
        result = process_lifecycle.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',script],capture_output=True,text=True,timeout=8,creationflags=subprocess.CREATE_NO_WINDOW)
        return min(100.0,max(0.0,float(result.stdout.strip()))) if result.returncode == 0 else None
    except (OSError, ValueError, subprocess.TimeoutExpired):
        return None


def reveal_in_explorer(path):
    """Select the exact item using Windows Shell PIDLs, with no command parsing."""
    if os.name!='nt':
        raise OSError('Explorer selection requires Windows.')
    item=Path(path).resolve()
    if not item.exists():
        raise FileNotFoundError('This file no longer exists: '+str(item))
    ole=ctypes.windll.ole32
    shell=ctypes.windll.shell32
    ole.CoInitialize.argtypes=[ctypes.c_void_p]
    ole.CoInitialize.restype=ctypes.c_long
    ole.CoTaskMemFree.argtypes=[ctypes.c_void_p]
    shell.SHParseDisplayName.argtypes=[ctypes.c_wchar_p,ctypes.c_void_p,ctypes.POINTER(ctypes.c_void_p),wintypes.DWORD,ctypes.POINTER(wintypes.DWORD)]
    shell.SHParseDisplayName.restype=ctypes.c_long
    shell.SHOpenFolderAndSelectItems.argtypes=[ctypes.c_void_p,wintypes.UINT,ctypes.c_void_p,wintypes.DWORD]
    shell.SHOpenFolderAndSelectItems.restype=ctypes.c_long
    initialized=ole.CoInitialize(None)
    pidl=ctypes.c_void_p()
    try:
        result=shell.SHParseDisplayName(str(item),None,ctypes.byref(pidl),0,None)
        if result<0:
            raise OSError('Windows could not locate the item in Explorer.')
        result=shell.SHOpenFolderAndSelectItems(pidl,0,None,0)
        if result<0:
            raise OSError('Windows could not select this item in Explorer.')
    finally:
        if pidl.value:
            ole.CoTaskMemFree(pidl)
        if initialized>=0:
            ole.CoUninitialize()
