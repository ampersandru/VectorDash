"""A disposable Win32 window for native control regressions. No installed app/user data."""
import ctypes as c, json, pathlib, sys
from ctypes import wintypes as w
u=c.WinDLL('user32',use_last_error=True);k=c.WinDLL('kernel32',use_last_error=True)
LRESULT=c.c_ssize_t;WNDPROC=c.WINFUNCTYPE(LRESULT,w.HWND,w.UINT,w.WPARAM,w.LPARAM)
class WC(c.Structure):_fields_=[('style',w.UINT),('proc',WNDPROC),('cbClsExtra',c.c_int),('cbWndExtra',c.c_int),('instance',w.HINSTANCE),('icon',w.HICON),('cursor',w.HANDLE),('background',w.HBRUSH),('menu',w.LPCWSTR),('name',w.LPCWSTR)]
k.GetModuleHandleW.argtypes=[w.LPCWSTR];k.GetModuleHandleW.restype=w.HMODULE
u.CreateWindowExW.argtypes=[w.DWORD,w.LPCWSTR,w.LPCWSTR,w.DWORD,c.c_int,c.c_int,c.c_int,c.c_int,w.HWND,w.HMENU,w.HINSTANCE,c.c_void_p];u.CreateWindowExW.restype=w.HWND
u.DefWindowProcW.argtypes=[w.HWND,w.UINT,w.WPARAM,w.LPARAM];u.DefWindowProcW.restype=LRESULT
u.GetWindowTextW.argtypes=[w.HWND,w.LPWSTR,c.c_int];u.SetWindowTextW.argtypes=[w.HWND,w.LPCWSTR]
u.DestroyWindow.argtypes=[w.HWND];u.RegisterClassW.argtypes=[c.POINTER(WC)]
u.GetMessageW.argtypes=[c.POINTER(w.MSG),w.HWND,w.UINT,w.UINT]
u.TranslateMessage.argtypes=[c.POINTER(w.MSG)];u.DispatchMessageW.argtypes=[c.POINTER(w.MSG)];u.DispatchMessageW.restype=LRESULT
state=pathlib.Path(sys.argv[1]);ready=pathlib.Path(sys.argv[2]);title=sys.argv[3]
edit=status=None
@WNDPROC
def proc(hwnd,msg,wp,lp):
 if msg==0x111 and (wp&0xffff)==102:
  text=c.create_unicode_buffer(4096);u.GetWindowTextW(edit,text,len(text));state.write_text(text.value,encoding='utf-8');u.SetWindowTextW(status,'Confirmed: '+text.value);return 0
 if msg==0x2:u.PostQuitMessage(0);return 0
 return u.DefWindowProcW(hwnd,msg,wp,lp)
instance=k.GetModuleHandleW(None);name='VectorDashNativeFixture'
wc=WC(0,proc,0,0,instance,None,None,w.HBRUSH(6),None,name);assert u.RegisterClassW(c.byref(wc))
hwnd=u.CreateWindowExW(0,name,title,0x10CF0000,80,80,620,280,None,None,instance,None);assert hwnd,c.get_last_error()
edit=u.CreateWindowExW(0x200,'EDIT','Input',0x50810080,20,25,450,32,hwnd,w.HMENU(101),instance,None)
button=u.CreateWindowExW(0,'BUTTON','Apply',0x50010000,20,75,110,32,hwnd,w.HMENU(102),instance,None)
status=u.CreateWindowExW(0,'STATIC','Waiting',0x50000000,20,130,550,55,hwnd,w.HMENU(103),instance,None)
ready.write_text(json.dumps({'hwnd':hwnd,'pid':__import__('os').getpid(),'title':title}),encoding='utf-8')
msg=w.MSG()
while u.GetMessageW(c.byref(msg),None,0,0)>0:u.TranslateMessage(c.byref(msg));u.DispatchMessageW(c.byref(msg))
