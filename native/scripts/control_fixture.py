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
u.GetDlgItem.argtypes=[w.HWND,c.c_int];u.GetDlgItem.restype=w.HWND
u.DestroyWindow.argtypes=[w.HWND];u.RegisterClassW.argtypes=[c.POINTER(WC)]
u.GetMessageW.argtypes=[c.POINTER(w.MSG),w.HWND,w.UINT,w.UINT]
u.TranslateMessage.argtypes=[c.POINTER(w.MSG)];u.DispatchMessageW.argtypes=[c.POINTER(w.MSG)];u.DispatchMessageW.restype=LRESULT
state=pathlib.Path(sys.argv[1]);ready=pathlib.Path(sys.argv[2]);title=sys.argv[3]
edit=status=toggle=None
modal_hwnd=None
confirmation_hwnd=None
u.SendMessageW.argtypes=[w.HWND,w.UINT,w.WPARAM,w.LPARAM];u.SendMessageW.restype=LRESULT
@WNDPROC
def proc(hwnd,msg,wp,lp):
 global modal_hwnd,confirmation_hwnd
 if msg==0x111 and (wp&0xffff)==105:
  modal_hwnd=u.CreateWindowExW(0,name,'VectorDash modal fixture',0x90C80000,160,160,420,230,hwnd,None,instance,None)
  u.EnableWindow(hwnd,False)
  u.CreateWindowExW(0,'BUTTON','Save',0x50010000,20,50,110,32,modal_hwnd,w.HMENU(1),instance,None)
  u.CreateWindowExW(0,'BUTTON','Save',0x50010000,180,50,110,32,modal_hwnd,w.HMENU(1),instance,None)
  u.CreateWindowExW(0,'BUTTON','Open confirmation',0x50010000,20,110,180,32,modal_hwnd,w.HMENU(106),instance,None)
  return 0
 if msg==0x111 and (wp&0xffff)==106:
  confirmation_hwnd=u.CreateWindowExW(0,name,'VectorDash confirmation fixture',0x90C80000,200,200,340,180,hwnd,None,instance,None);u.EnableWindow(hwnd,False)
  u.CreateWindowExW(0,'BUTTON','Dismiss confirmation',0x50010000,20,45,220,32,confirmation_hwnd,w.HMENU(107),instance,None);return 0
 if msg==0x111 and (wp&0xffff)==107:
  u.DestroyWindow(hwnd);return 0
 if msg==0x111 and (wp&0xffff)==1 and hwnd==modal_hwnd:
  state.write_text('modal second clicked' if lp != u.GetDlgItem(hwnd,1) else 'modal first clicked',encoding='utf-8');u.DestroyWindow(hwnd);return 0
 if msg==0x111 and (wp&0xffff)>=500:
  state.write_text('late clicked '+str(wp&0xffff),encoding='utf-8');return 0
 if msg==0x111 and (wp&0xffff)==102:
  text=c.create_unicode_buffer(4096);u.GetWindowTextW(edit,text,len(text));state.write_text(text.value,encoding='utf-8');u.SetWindowTextW(status,'Confirmed: '+text.value);return 0
 if msg==0x111 and (wp&0xffff)==104:
  state.write_text('on' if u.SendMessageW(toggle,0xF0,0,0) else 'off',encoding='utf-8');return 0
 if msg==0x2:
  if hwnd==confirmation_hwnd:
   confirmation_hwnd=None;u.EnableWindow(modal_hwnd,True);return 0
  if hwnd==modal_hwnd:
   modal_hwnd=None;u.EnableWindow(root_hwnd,True);return 0
  u.PostQuitMessage(0);return 0
 return u.DefWindowProcW(hwnd,msg,wp,lp)
instance=k.GetModuleHandleW(None);name='VectorDashNativeFixture'
wc=WC(0,proc,0,0,instance,None,None,w.HBRUSH(6),None,name);assert u.RegisterClassW(c.byref(wc))
hwnd=u.CreateWindowExW(0,name,title,0x10CF0000,80,80,620,280,None,None,instance,None);assert hwnd,c.get_last_error()
root_hwnd=hwnd
edit_class='EDIT'
if '--keyboard-input' in sys.argv:
 u.GetClassInfoW.argtypes=[w.HINSTANCE,w.LPCWSTR,c.POINTER(WC)]
 original=WC();assert u.GetClassInfoW(None,'EDIT',c.byref(original))
 original.instance=instance;original.name='VectorDashKeyboardEdit';assert u.RegisterClassW(c.byref(original))
 edit_class='VectorDashKeyboardEdit'
edit=u.CreateWindowExW(0x200,edit_class,'Input',0x50810080 | (0x44 if '--keyboard-input' in sys.argv else 0),20,25,450,32,hwnd,w.HMENU(101),instance,None)
button=u.CreateWindowExW(0,'BUTTON','Apply',0x50010000,20,75,110,32,hwnd,w.HMENU(102),instance,None)
status=u.CreateWindowExW(0,'STATIC','Waiting',0x50000000,20,130,550,55,hwnd,w.HMENU(103),instance,None)
if '--toggle' in sys.argv:
 toggle=u.CreateWindowExW(0,'BUTTON','Test switch',0x50010003,160,75,180,32,hwnd,w.HMENU(104),instance,None)
 u.SendMessageW(toggle,0xF1,1,0);state.write_text('on',encoding='utf-8')
u.EnableWindow.argtypes=[w.HWND,w.BOOL]
u.CreateWindowExW(0,'BUTTON','Open dialog',0x50010000,350,75,160,32,hwnd,w.HMENU(105),instance,None)
if '--navigation' in sys.argv:
 box=u.CreateWindowExW(0x200,'LISTBOX','Scrollable list',0x50200001,20,160,260,90,hwnd,w.HMENU(110),instance,None)
 for i in range(100):
  item=c.create_unicode_buffer('Scroll item '+str(i));u.SendMessageW(box,0x180,0,c.addressof(item))
 for i in range(240):u.CreateWindowExW(0,'BUTTON','Late control '+str(i),0x50010000,350,180,180,28,hwnd,w.HMENU(500+i),instance,None)
if '--tabs' in sys.argv:
 class INIT(c.Structure): _fields_=[('size',w.DWORD),('classes',w.DWORD)]
 init=INIT(c.sizeof(INIT),8);assert c.WinDLL('comctl32').InitCommonControlsEx(c.byref(init))
 tabs=u.CreateWindowExW(0,'SysTabControl32','Release types',0x50010000,20,170,450,55,hwnd,w.HMENU(120),instance,None);assert tabs
 class ITEM(c.Structure): _fields_=[('mask',w.UINT),('state',w.DWORD),('stateMask',w.DWORD),('text',w.LPWSTR),('length',c.c_int),('image',c.c_int),('param',w.LPARAM)]
 for index,label in enumerate(['Albums','Singles and EPs']):
  buffer=c.create_unicode_buffer(label);item=ITEM();item.mask=1;item.text=c.cast(buffer,w.LPWSTR);assert u.SendMessageW(tabs,0x133e,index,c.addressof(item))==index
ready.write_text(json.dumps({'hwnd':hwnd,'pid':__import__('os').getpid(),'title':title}),encoding='utf-8')
msg=w.MSG()
while u.GetMessageW(c.byref(msg),None,0,0)>0:u.TranslateMessage(c.byref(msg));u.DispatchMessageW(c.byref(msg))
