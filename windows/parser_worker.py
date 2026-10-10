"""Bounded, low-integrity Windows worker for untrusted document/media parsing."""
import base64
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time
import process_lifecycle

_handles=[]
def constrain_worker():
    if os.name!='nt': return
    kernel=ctypes.WinDLL('kernel32',use_last_error=True)
    advapi=ctypes.WinDLL('advapi32',use_last_error=True)
    kernel.GetCurrentProcess.restype=wintypes.HANDLE
    advapi.OpenProcessToken.argtypes=[wintypes.HANDLE,wintypes.DWORD,ctypes.POINTER(wintypes.HANDLE)]
    advapi.ConvertStringSidToSidW.argtypes=[wintypes.LPCWSTR,ctypes.POINTER(ctypes.c_void_p)]
    advapi.SetTokenInformation.argtypes=[wintypes.HANDLE,ctypes.c_int,ctypes.c_void_p,wintypes.DWORD]
    kernel.CloseHandle.argtypes=[wintypes.HANDLE]
    kernel.LocalFree.argtypes=[ctypes.c_void_p]
    class Basic(ctypes.Structure):
        _fields_=[('ProcessTime',ctypes.c_longlong),('JobTime',ctypes.c_longlong),('Flags',wintypes.DWORD),('MinWorking',ctypes.c_size_t),('MaxWorking',ctypes.c_size_t),('ActiveProcesses',wintypes.DWORD),('Affinity',ctypes.c_size_t),('Priority',wintypes.DWORD),('Scheduling',wintypes.DWORD)]
    class IO(ctypes.Structure): _fields_=[(name,ctypes.c_ulonglong) for name in ('ReadOps','WriteOps','OtherOps','ReadBytes','WriteBytes','OtherBytes')]
    class Limits(ctypes.Structure): _fields_=[('Basic',Basic),('IO',IO),('ProcessMemory',ctypes.c_size_t),('JobMemory',ctypes.c_size_t),('PeakProcess',ctypes.c_size_t),('PeakJob',ctypes.c_size_t)]
    kernel.CreateJobObjectW.argtypes=[ctypes.c_void_p,wintypes.LPCWSTR];kernel.CreateJobObjectW.restype=wintypes.HANDLE
    kernel.SetInformationJobObject.argtypes=[wintypes.HANDLE,ctypes.c_int,ctypes.c_void_p,wintypes.DWORD]
    kernel.AssignProcessToJobObject.argtypes=[wintypes.HANDLE,wintypes.HANDLE]
    job=kernel.CreateJobObjectW(None,None)
    limits=Limits();limits.Basic.Flags=0x100|0x8|0x2000;limits.Basic.ActiveProcesses=4;limits.ProcessMemory=1024*1024*1024
    if not job or not kernel.SetInformationJobObject(job,9,ctypes.byref(limits),ctypes.sizeof(limits)) or not kernel.AssignProcessToJobObject(job,kernel.GetCurrentProcess()):
        raise ctypes.WinError(ctypes.get_last_error())
    _handles.append(job)
    token=wintypes.HANDLE();sid=ctypes.c_void_p()
    class Label(ctypes.Structure): _fields_=[('Sid',ctypes.c_void_p),('Attributes',wintypes.DWORD)]
    if not advapi.OpenProcessToken(kernel.GetCurrentProcess(),0x80|0x20|0x8,ctypes.byref(token)):raise ctypes.WinError(ctypes.get_last_error())
    try:
        advapi.AdjustTokenPrivileges.argtypes=[wintypes.HANDLE,wintypes.BOOL,ctypes.c_void_p,wintypes.DWORD,ctypes.c_void_p,ctypes.c_void_p]
        if not advapi.AdjustTokenPrivileges(token,True,None,0,None,None):raise ctypes.WinError(ctypes.get_last_error())
        if not advapi.ConvertStringSidToSidW('S-1-16-4096',ctypes.byref(sid)):raise ctypes.WinError(ctypes.get_last_error())
        label=Label(sid,0x20)
        if not advapi.SetTokenInformation(token,25,ctypes.byref(label),ctypes.sizeof(label)+12):raise ctypes.WinError(ctypes.get_last_error())
    finally:
        kernel.CloseHandle(token)
        if sid:kernel.LocalFree(sid)


def worker_main():
    try:
        request=json.loads(sys.stdin.readline(1024*1024))
        constrain_worker()
        from content import extract,code_segments
        from vectordash_core import read_text
        segments=code_segments(Path(request['path'])) if request.get('code') else extract(Path(request['path']),request['settings'],read_text,lambda:None)
        for segment in segments:
            payload=vars(segment).copy();payload['media']=base64.b64encode(segment.media).decode()
            sys.stdout.write(json.dumps({'segment':payload},ensure_ascii=True)+'\n');sys.stdout.flush()
        return 0
    except Exception as error:
        sys.stdout.write(json.dumps({'error':str(error)+' ['+__import__('traceback').format_exc().splitlines()[-3]+']'})+'\n');sys.stdout.flush()
        return 1

def isolated_extract(path,settings,checkpoint,code=False):
    from content import Segment,extract
    from vectordash_core import read_text,TEXT
    if path.suffix.lower() in TEXT and not code:
        yield from extract(path,settings,read_text,checkpoint)
        return
    supported={'.pdf','.docx','.xlsx','.pptx','.zip','.safetensors'}
    from content import IMAGE,AUDIO,VIDEO
    if path.suffix.lower() not in supported|IMAGE|AUDIO|VIDEO and not code:
        yield Segment(kind='filename');return
    if getattr(sys,'frozen',False): command=[str(Path(sys.executable).with_name('VectorDash-Worker.exe')),'--extract-worker']
    else: command=[sys.executable,str(Path(__file__).with_name('desktop.py')),'--extract-worker']
    proc=process_lifecycle.popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
    messages=queue.Queue(maxsize=4)
    done=threading.Event()
    def read():
        try:
            while not done.is_set():
                line=proc.stdout.readline(24*1024*1024+1)
                if not line:break
                while not done.is_set():
                    try:messages.put(line,timeout=.1);break
                    except queue.Full:pass
        finally:
            while not done.is_set():
                try:messages.put(None,timeout=.1);break
                except queue.Full:pass
    reader=threading.Thread(target=read,daemon=True);reader.start()
    try:
        options={key:value for key,value in settings.items() if key not in ('remote_key','mcp_token')}
        proc.stdin.write((json.dumps({'path':str(path),'settings':options,'code':code})+'\n').encode());proc.stdin.close()
        deadline=time.monotonic()+900
        while True:
            checkpoint()
            if time.monotonic()>deadline:raise TimeoutError('Parser exceeded 15-minute limit')
            try:line=messages.get(timeout=.25)
            except queue.Empty:continue
            if line is None:break
            if len(line)>24*1024*1024:raise ValueError('Parser output too large')
            message=json.loads(line)
            if 'error' in message:raise ValueError(message['error'])
            data=message['segment'];data['media']=base64.b64decode(data['media'],validate=True)
            yield Segment(**data)
        if proc.wait(timeout=5):raise RuntimeError('Document parser stopped unexpectedly')
    finally:
        done.set()
        if proc.poll() is None:proc.kill()
        proc.wait()
        reader.join(timeout=2)
        proc.stdout.close()

