"""Read Windows Shell icons into QImage on a worker, without launching programs."""
import ctypes
from ctypes import wintypes as w
import os
from PySide6.QtGui import QImage

class ShellInfo(ctypes.Structure):
    _fields_=[('icon',w.HICON),('index',ctypes.c_int),('attributes',w.DWORD),('display',w.WCHAR*260),('type',w.WCHAR*80)]
class BitmapHeader(ctypes.Structure):
    _fields_=[('size',w.DWORD),('width',w.LONG),('height',w.LONG),('planes',w.WORD),('bits',w.WORD),('compression',w.DWORD),('image_size',w.DWORD),('x',w.LONG),('y',w.LONG),('used',w.DWORD),('important',w.DWORD)]

def load_icon(path,size=64):
    if os.name!='nt':return None
    ole=ctypes.WinDLL('ole32');shell=ctypes.WinDLL('shell32');user=ctypes.WinDLL('user32');gdi=ctypes.WinDLL('gdi32')
    ole.CoInitializeEx.argtypes=[ctypes.c_void_p,w.DWORD];ole.CoInitializeEx.restype=ctypes.c_long
    initialized=ole.CoInitializeEx(None,2)>=0
    pidl=ctypes.c_void_p();icon=dc=bitmap=old=None
    shell.SHParseDisplayName.argtypes=[w.LPCWSTR,ctypes.c_void_p,ctypes.POINTER(ctypes.c_void_p),w.DWORD,ctypes.c_void_p]
    shell.SHParseDisplayName.restype=ctypes.c_long
    shell.SHGetFileInfoW.argtypes=[ctypes.c_void_p,w.DWORD,ctypes.POINTER(ShellInfo),w.UINT,w.UINT];shell.SHGetFileInfoW.restype=ctypes.c_size_t
    gdi.CreateCompatibleDC.argtypes=[w.HDC];gdi.CreateCompatibleDC.restype=w.HDC
    gdi.CreateDIBSection.argtypes=[w.HDC,ctypes.POINTER(BitmapHeader),w.UINT,ctypes.POINTER(ctypes.c_void_p),w.HANDLE,w.DWORD];gdi.CreateDIBSection.restype=w.HBITMAP
    gdi.SelectObject.argtypes=[w.HDC,w.HANDLE];gdi.SelectObject.restype=w.HANDLE
    gdi.DeleteObject.argtypes=[w.HANDLE];gdi.DeleteDC.argtypes=[w.HDC]
    user.DrawIconEx.argtypes=[w.HDC,ctypes.c_int,ctypes.c_int,w.HICON,ctypes.c_int,ctypes.c_int,w.UINT,w.HBRUSH,w.UINT]
    user.DestroyIcon.argtypes=[w.HICON]
    ole.CoTaskMemFree.argtypes=[ctypes.c_void_p]
    try:
        flags=0x100
        if path.startswith('shell:AppsFolder\\'):
            from programs import APP_ID
            if not APP_ID.fullmatch(path.split('\\',1)[1]):return None
            canonical='shell:::{4234d49b-0245-4df3-b780-3893943456e1}\\'+path.split('\\',1)[1]
            if shell.SHParseDisplayName(canonical,None,ctypes.byref(pidl),0,None)<0:return None
            target=pidl;flags|=0x8
        else:target=ctypes.cast(ctypes.c_wchar_p(path),ctypes.c_void_p)
        info=ShellInfo()
        if not shell.SHGetFileInfoW(target,0,ctypes.byref(info),ctypes.sizeof(info),flags):return None
        icon=info.icon
        dc=gdi.CreateCompatibleDC(None)
        header=BitmapHeader(ctypes.sizeof(BitmapHeader),size,-size,1,32,0,size*size*4,0,0,0,0)
        bits=ctypes.c_void_p();bitmap=gdi.CreateDIBSection(dc,ctypes.byref(header),0,ctypes.byref(bits),None,0)
        if not dc or not bitmap:return None
        old=gdi.SelectObject(dc,bitmap)
        length=size*size*4;ctypes.memset(bits,0,length)
        if not user.DrawIconEx(dc,0,0,icon,size,size,0,None,3):return None
        black=bytearray(ctypes.string_at(bits,length));ctypes.memset(bits,255,length)
        if not user.DrawIconEx(dc,0,0,icon,size,size,0,None,3):return None
        white=ctypes.string_at(bits,length)
        for offset in range(0,length,4):
            black[offset+3]=255-max(0,min(255,max(white[offset+i]-black[offset+i] for i in range(3))))
        return QImage(bytes(black),size,size,size*4,QImage.Format.Format_ARGB32_Premultiplied).copy()
    finally:
        if old:gdi.SelectObject(dc,old)
        if bitmap:gdi.DeleteObject(bitmap)
        if dc:gdi.DeleteDC(dc)
        if icon:user.DestroyIcon(icon)
        if pidl:ole.CoTaskMemFree(pidl)
        if initialized:ole.CoUninitialize()
