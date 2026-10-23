"""Real Windows Save As selection test; never writes or overwrites a selected file."""
import ctypes as c,json,pathlib,subprocess,sys,time,uuid,queue,threading,argparse
from ctypes import wintypes as w
if len(sys.argv)>1 and sys.argv[1]=='--dialog':
 root=pathlib.Path(sys.argv[2]);title=sys.argv[3]
 class OFN(c.Structure):
  _fields_=[('size',w.DWORD),('owner',w.HWND),('instance',w.HINSTANCE),('filter',w.LPCWSTR),('custom',w.LPWSTR),('maxcustom',w.DWORD),('index',w.DWORD),('file',w.LPWSTR),('maxfile',w.DWORD),('filetitle',w.LPWSTR),('maxfiletitle',w.DWORD),('directory',w.LPCWSTR),('title',w.LPCWSTR),('flags',w.DWORD),('offset',w.WORD),('extension',w.WORD),('defext',w.LPCWSTR),('data',w.LPARAM),('hook',c.c_void_p),('template',w.LPCWSTR),('reserved',c.c_void_p),('reservedint',w.DWORD),('flagsex',w.DWORD)]
 name=c.create_unicode_buffer('original.txt',4096);o=OFN();o.size=c.sizeof(o);o.file=c.cast(name,w.LPWSTR);o.maxfile=4096;o.directory=str(root);o.title=title;o.filter='Text files\0*.txt\0\0';o.defext='txt';o.flags=0x80000|0x800|0x8|0x2
 dll=c.WinDLL('comdlg32',use_last_error=True);dll.GetSaveFileNameW.argtypes=[c.POINTER(OFN)];dll.GetSaveFileNameW.restype=w.BOOL
 ok=dll.GetSaveFileNameW(c.byref(o));(root/'result.json').write_text(json.dumps({'accepted':bool(ok),'path':name.value}),encoding='utf-8');sys.exit(0)
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');a=p.parse_args()
root=pathlib.Path('windows/.smoke/control-save').resolve()/uuid.uuid4().hex;root.mkdir(parents=True);original=root/'original.txt';original.write_text('Original must remain intact',encoding='utf-8');title='VectorDash Save As fixture '+uuid.uuid4().hex[:8]
dialog=subprocess.Popen([sys.executable,__file__,'--dialog',str(root),title],stdout=subprocess.DEVNULL,stderr=(root/'dialog.log').open('w'));worker=None
try:
 worker=subprocess.Popen([str(pathlib.Path(a.exe).resolve()),'--control-worker'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,encoding='utf-8');responses=queue.Queue()
 def reader():
  for line in worker.stdout:responses.put(line)
 threading.Thread(target=reader,daemon=True).start()
 def call(action,**args):
  worker.stdin.write(json.dumps({'tool':'computer_use','args':{'action':action,**args},'vision':False,'owner_pid':0})+'\n');worker.stdin.flush();result=json.loads(responses.get(timeout=25));
  with (root/'trace.jsonl').open('a',encoding='utf-8') as log:log.write(json.dumps({'action':action,'args':args,'result':result})+'\n')
  return result
 def good(v):assert 'error' not in v,{k:x for k,x in v.items() if k!='image_url'};return v
 deadline=time.monotonic()+10;wid=None
 while time.monotonic()<deadline:
  assert dialog.poll() is None,(root/'dialog.log').read_text()
  win=good(call('windows'));wid=next((v['window_id'] for v in win['windows'] if v['title']==title),None)
  if wid:break
  time.sleep(.1)
 assert wid,'Save dialog did not appear'
 snap=good(call('inspect',window_id=wid));field=next((v for v in snap['elements'] if v.get('value')=='original.txt' and v.get('role')=='edit' and 'file name' in v.get('name','').lower()),None)
 assert field,[(v.get('name'),v.get('role'),v.get('automation_id'),v.get('value')) for v in snap['elements']]
 snap=good(call('type',window_id=wid,snapshot_id=snap['snapshot_id'],element=field['id'],text_mode='replace_all',text='replacement.txt'))
 save=next(v for v in snap['elements'] if v.get('role')=='button' and v.get('name','').replace('&','')=='Save')
 call('click',window_id=wid,snapshot_id=snap['snapshot_id'],element=save['id']) # Closing the top-level dialog can invalidate its post-action observation.
 deadline=time.monotonic()+15
 while not (root/'result.json').exists() and time.monotonic()<deadline:time.sleep(.05)
 if not (root/'result.json').exists():
  call('inspect',window_id=wid);win=call('windows');print('Remaining fixture windows:',[v for v in win.get('windows',[]) if v.get('application','').lower()=='python.exe'],flush=True)
 assert (root/'result.json').exists(),f'Save selection did not complete; dialog exit={dialog.poll()}, error={(root/"dialog.log").read_text()}'
 result=json.loads((root/'result.json').read_text(encoding='utf-8'));assert result['accepted'] and pathlib.Path(result['path'])==root/'replacement.txt',result
 assert original.read_text(encoding='utf-8')=='Original must remain intact' and not (root/'replacement.txt').exists()
 print('PASS real Windows Save As: requested replacement.txt returned; existing original.txt untouched; selected file was never written',flush=True)
 print('Artifacts:',root,flush=True)
finally:
 if worker:worker.terminate();worker.wait()
 if dialog.poll() is None:dialog.terminate()
 dialog.wait()
