"""Clipboard-output checks for area capture; restores prior memory-backed clipboard formats."""
import argparse,ctypes as c,io,json,os,pathlib,subprocess,time,uuid
from PIL import Image,ImageGrab
p=argparse.ArgumentParser();p.add_argument('--exe',required=True);a=p.parse_args()
u=c.WinDLL('user32');k=c.WinDLL('kernel32')
u.OpenClipboard.argtypes=[c.c_void_p];u.GetClipboardData.argtypes=[c.c_uint];u.GetClipboardData.restype=c.c_void_p
u.SetClipboardData.argtypes=[c.c_uint,c.c_void_p];u.SetClipboardData.restype=c.c_void_p
k.GlobalSize.argtypes=[c.c_void_p];k.GlobalSize.restype=c.c_size_t;k.GlobalLock.argtypes=[c.c_void_p];k.GlobalLock.restype=c.c_void_p
k.GlobalUnlock.argtypes=[c.c_void_p];k.GlobalAlloc.argtypes=[c.c_uint,c.c_size_t];k.GlobalAlloc.restype=c.c_void_p;k.GlobalFree.argtypes=[c.c_void_p]
def opened():
 for _ in range(50):
  if u.OpenClipboard(None):return
  time.sleep(.02)
 raise RuntimeError('Clipboard is busy')
def snapshot():
 opened();out={};formats=[];fmt=0
 try:
  while True:
   fmt=u.EnumClipboardFormats(fmt)
   if not fmt:break
   formats.append(fmt)
   if fmt in [2,3,9,14,0x82,0x83,0x89,0x8e]:continue
   handle=u.GetClipboardData(fmt);size=k.GlobalSize(handle)
   if size:
    assert size<=32*1024*1024,'Clipboard is too large for a safe fixture backup'
    ptr=k.GlobalLock(handle)
    if ptr:out[fmt]=c.string_at(ptr,size);k.GlobalUnlock(handle)
  assert not any(f in formats for f in [3,9,14,0x83,0x89,0x8e]),'Cannot losslessly back up this clipboard object'
  assert 2 not in formats or 8 in out or 17 in out,'Cannot losslessly back up bitmap-only clipboard'
  return out
 finally:u.CloseClipboard()
def restore(data):
 opened()
 try:
  assert u.EmptyClipboard()
  for fmt,raw in data.items():
   handle=k.GlobalAlloc(0x42,len(raw));assert handle
   ptr=k.GlobalLock(handle);c.memmove(ptr,raw,len(raw));k.GlobalUnlock(handle)
   if not u.SetClipboardData(fmt,handle):k.GlobalFree(handle);raise RuntimeError('Clipboard restore failed')
 finally:u.CloseClipboard()
root=pathlib.Path('windows/.smoke/capture-clipboard').resolve()/uuid.uuid4().hex;root.mkdir(parents=True)
fixture=root/'capture.png';Image.new('RGB',(2000,1200),(35,155,205)).save(fixture)
backup=snapshot()
try:
 for name,enabled,flags,valid in [('manual-main',True,['--smoke-manual-area'],True),('manual-popup',True,['--smoke-manual-area','--smoke-quick'],True),('shortcut',True,['--smoke-shortcut','capture_ai'],True),('disabled',False,['--smoke-manual-area'],True),('failed-capture',True,['--smoke-manual-area'],False)]:
  sentinel='Native capture clipboard fixture '+uuid.uuid4().hex;restore({13:(sentinel+'\0').encode('utf-16-le')})
  folder=root/name;profile=folder/'profile';profile.mkdir(parents=True)
  config={'encrypt_index':False,'roots':[],'include_programs':False,'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'language':'en','llm_enabled':True,'llm_vision':True,'llm_model':'fixture','clipboard_enabled':False,'ai_capture_clipboard':enabled}
  (profile/'settings.json').write_text(json.dumps(config),encoding='utf-8')
  subprocess.run([str(pathlib.Path(a.exe).resolve()),'--smoke-report',str(folder/'report.json'),'--smoke-seconds','3','--smoke-keep-open','--smoke-tool','AI','--smoke-capture-fixture',str(fixture if valid else root/'missing.png'),*flags],env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True,timeout=20)
  r=json.loads((folder/'report.json').read_text(encoding='utf-8'))
  assert r['manager']['capture_attached']==valid,r
  if enabled and valid:
   image=ImageGrab.grabclipboard();assert isinstance(image,Image.Image) and image.size==(2000,1200),getattr(image,'size',None)
   assert image.convert('RGB').getpixel((100,100))==(35,155,205)
  else:assert snapshot()[13].decode('utf-16-le').rstrip('\0')==sentinel
  print('PASS',name,flush=True)
 print('PASS five capture clipboard cases; original image dimensions and pixels verified',flush=True)
finally:restore(backup)
