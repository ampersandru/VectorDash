"""Windows account-bound protection and conservative content exclusions."""
from functools import lru_cache
import base64
import hashlib
import struct
import ctypes
from ctypes import wintypes
import os
from pathlib import Path
import re

MAGIC=b'VDPROTECTED1\0'
class Blob(ctypes.Structure):
    _fields_=[('size',wintypes.DWORD),('data',ctypes.POINTER(ctypes.c_ubyte))]

def crypt(data, decrypt=False):
    if os.name!='nt':
        raise OSError('Windows account encryption requires Windows')
    buffer=ctypes.create_string_buffer(data)
    # Cast the address, not the owning buffer: ctypes otherwise creates an
    # ownership cycle that retains multi-MB DPAPI inputs until cyclic GC runs.
    # The local buffer remains alive until the synchronous Windows call returns.
    source=Blob(len(data),ctypes.cast(ctypes.addressof(buffer),ctypes.POINTER(ctypes.c_ubyte)))
    target=Blob()
    library=ctypes.WinDLL('crypt32',use_last_error=True)
    function=library.CryptUnprotectData if decrypt else library.CryptProtectData
    function.argtypes=[ctypes.POINTER(Blob),ctypes.c_void_p,ctypes.c_void_p,ctypes.c_void_p,ctypes.c_void_p,wintypes.DWORD,ctypes.POINTER(Blob)]
    function.restype=wintypes.BOOL
    if not function(ctypes.byref(source),None,None,None,None,1,ctypes.byref(target)):
        raise ctypes.WinError(ctypes.get_last_error())
    try:return ctypes.string_at(target.data,target.size)
    finally:
        free=ctypes.WinDLL('kernel32').LocalFree
        free.argtypes=[ctypes.c_void_p];free.restype=ctypes.c_void_p
        free(target.data)

CHUNK_MARKER=b'VDCHUNK1'
CHUNK_SIZE=8*1024*1024

def seal(data):
    # Limit individual DPAPI calls for large indexes; old single-block files work.
    if len(data)<=CHUNK_SIZE:return MAGIC+crypt(data)
    count=(len(data)+CHUNK_SIZE-1)//CHUNK_SIZE
    digest=hashlib.sha256(data).digest()
    output=bytearray(MAGIC+CHUNK_MARKER+struct.pack('<I',count))
    for number,offset in enumerate(range(0,len(data),CHUNK_SIZE)):
        protected=crypt(struct.pack('<II',number,count)+digest+data[offset:offset+CHUNK_SIZE])
        output.extend(struct.pack('<I',len(protected)));output.extend(protected)
    return bytes(output)

def unseal(data):
    if not data.startswith(MAGIC):raise ValueError('Invalid protected index')
    payload=data[len(MAGIC):]
    if not payload.startswith(CHUNK_MARKER):return crypt(payload,True)
    if len(payload)<12:raise ValueError('Truncated protected index')
    count=struct.unpack_from('<I',payload,8)[0]
    if count<2 or count>len(payload)//4:raise ValueError('Invalid protected chunk count')
    offset=12;chunks=[];digest=None
    for number in range(count):
        if len(payload)-offset<4:raise ValueError('Truncated protected index')
        size=struct.unpack_from('<I',payload,offset)[0];offset+=4
        if size>len(payload)-offset:raise ValueError('Truncated protected index')
        chunk=crypt(payload[offset:offset+size],True);offset+=size
        if len(chunk)<40 or struct.unpack_from('<II',chunk)!=(number,count):raise ValueError('Invalid protected chunk order')
        if digest is None:digest=chunk[8:40]
        if chunk[8:40]!=digest:raise ValueError('Mixed protected chunks')
        chunks.append(chunk[40:])
    result=b''.join(chunks)
    if offset!=len(payload) or hashlib.sha256(result).digest()!=digest:raise ValueError('Protected index integrity mismatch')
    return result

def seal_to_file(data,path,checkpoint=lambda:None):
    """Persist DPAPI chunks directly, without a second whole encrypted buffer."""
    view=memoryview(data);count=(len(view)+CHUNK_SIZE-1)//CHUNK_SIZE
    with open(path,'wb') as stream:
        if count<=1:
            checkpoint();stream.write(MAGIC+crypt(bytes(view)));return
        digest=hashlib.sha256(view).digest()
        stream.write(MAGIC+CHUNK_MARKER+struct.pack('<I',count))
        for number,offset in enumerate(range(0,len(view),CHUNK_SIZE)):
            checkpoint()
            protected=crypt(struct.pack('<II',number,count)+digest+bytes(view[offset:offset+CHUNK_SIZE]))
            stream.write(struct.pack('<I',len(protected)));stream.write(protected)

def unseal_file(path):
    """Read protected chunks into one plain buffer; never write plain content."""
    with open(path,'rb') as stream:
        if stream.read(len(MAGIC))!=MAGIC:raise ValueError('Invalid protected index')
        marker=stream.read(len(CHUNK_MARKER))
        if marker!=CHUNK_MARKER:return crypt(marker+stream.read(),True)
        raw=stream.read(4)
        if len(raw)!=4:raise ValueError('Truncated protected index')
        count=struct.unpack('<I',raw)[0]
        if count<2 or count>os.path.getsize(path)//4:raise ValueError('Invalid protected chunk count')
        result=bytearray();digest=None;hasher=hashlib.sha256()
        for number in range(count):
            raw=stream.read(4)
            if len(raw)!=4:raise ValueError('Truncated protected index')
            size=struct.unpack('<I',raw)[0]
            if size>CHUNK_SIZE+1024*1024:raise ValueError('Invalid protected chunk size')
            payload=stream.read(size)
            if len(payload)!=size:raise ValueError('Truncated protected index')
            chunk=crypt(payload,True)
            if len(chunk)<40 or struct.unpack_from('<II',chunk)!=(number,count):raise ValueError('Invalid protected chunk order')
            if digest is None:digest=chunk[8:40]
            if chunk[8:40]!=digest:raise ValueError('Mixed protected chunks')
            content=memoryview(chunk)[40:];hasher.update(content);result.extend(content)
        if stream.read(1) or hasher.digest()!=digest:raise ValueError('Protected index integrity mismatch')
        return result

def protect_secret(value):
    return 'dpapi:'+base64.b64encode(crypt(value.encode())).decode() if value and os.name=='nt' else value

def reveal_secret(value):
    return crypt(base64.b64decode(value[6:]),True).decode() if value.startswith('dpapi:') else value

SECRET_PATTERNS=[
    re.compile(r'-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----'),
    re.compile(r'\b(?:AKIA|ASIA)[A-Z0-9]{16}\b'),
    re.compile(r'\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{30,}|sk-[A-Za-z0-9_-]{20,})\b'),
    re.compile(r'(?i)\b(?:password|passwd|api[_ -]?key|client[_ -]?secret|access[_ -]?token|authorization)\b\s*[:=]\s*[\"\x27]?(?:Bearer\s+)?[A-Za-z0-9_+/=-]{8,}'),
    re.compile(r'\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b'),
]
def contains_credentials(text):return any(pattern.search(text) for pattern in SECRET_PATTERNS)

@lru_cache(maxsize=1)
def windows_churn_folders():
    system=Path(os.environ.get('SystemRoot',r'C:\Windows'))
    data=Path(os.environ.get('PROGRAMDATA',r'C:\ProgramData'))
    roots=[system,data/'Package Cache',data/'Microsoft/Windows Defender',data/'Microsoft/Windows/WER',data/'Microsoft/Windows/Caches']
    if os.environ.get('LOCALAPPDATA'):
        local=Path(os.environ['LOCALAPPDATA'])
        roots.extend(local/folder for folder in ('Temp','CrashDumps','Microsoft/Windows/INetCache','Microsoft/Windows/WebCache'))
    roots.append(data/'Microsoft/Windows/DeliveryOptimization')
    for name in ('TEMP','TMP'):
        if os.environ.get(name):roots.append(Path(os.environ[name]))
    for drive in 'ABCDEFGHIJKLMNOPQRSTUVWXYZ':
        for name in ('$RECYCLE.BIN','System Volume Information','$WINDOWS.~BT','$Windows.~WS','$WinREAgent','Config.Msi'):
            roots.append(Path(drive+':\\')/name)
    return [os.path.normcase(os.path.abspath(root)) for root in roots]

def excluded_path(path,settings):
    suffixes={s.lower() if s.startswith('.') else '.'+s.lower() for s in re.split(r'[,;\s]+',settings.get('index_excluded_types','')) if s}
    if Path(path).suffix.lower() in suffixes:return True
    folders=settings.get('excluded_folders',[])
    if settings.get('exclude_windows_churn'):folders=folders+windows_churn_folders()
    if not folders:return False
    absolute=Path(os.path.normcase(os.path.abspath(path)))
    for folder in folders:
        if absolute.is_relative_to(Path(os.path.normcase(os.path.abspath(folder)))):return True
    return False
