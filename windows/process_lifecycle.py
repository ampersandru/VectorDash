"""Gate helper process creation during application shutdown."""
import ctypes
import os
import sys
import subprocess
import threading

_lock=threading.RLock()
_closing=False

def reopen():
    global _closing
    with _lock:
        _closing=False

def close():
    global _closing
    with _lock:
        _closing=True

def popen(*args,**kwargs):
    with _lock:
        if _closing:
            raise InterruptedError('VectorDash is exiting')
        frozen=os.name=='nt' and getattr(sys,'frozen',False)
        if frozen:
            ctypes.windll.kernel32.SetDllDirectoryW.argtypes=[ctypes.c_wchar_p]
            ctypes.windll.kernel32.SetDllDirectoryW(None)
        try:
            return subprocess.Popen(*args,**kwargs)
        finally:
            if frozen:
                ctypes.windll.kernel32.SetDllDirectoryW(str(sys._MEIPASS))

def run(*args,**kwargs):
    # subprocess.run enters Popen immediately. Holding this lock for its entire
    # wait would block shutdown, so use its native wait/timeout through a guarded
    # Popen context, preserving capture_output, input and check semantics.
    input_data=kwargs.pop('input',None)
    capture=kwargs.pop('capture_output',False)
    timeout=kwargs.pop('timeout',None)
    check=kwargs.pop('check',False)
    if capture:
        kwargs['stdout']=subprocess.PIPE
        kwargs['stderr']=subprocess.PIPE
    if input_data is not None:
        kwargs['stdin']=subprocess.PIPE
    with popen(*args,**kwargs) as process:
        try:
            stdout,stderr=process.communicate(input_data,timeout=timeout)
        except BaseException:
            process.kill()
            process.wait()
            raise
        result=subprocess.CompletedProcess(process.args,process.returncode,stdout,stderr)
        if check:
            result.check_returncode()
        return result
