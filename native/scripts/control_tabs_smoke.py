"""Disposable native tab selection regression fixture."""
import argparse,ctypes,json,pathlib,queue,subprocess,sys,threading,time,uuid
from ctypes import wintypes as w
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');p.add_argument('--require-keyboard',action='store_true');a=p.parse_args()
root=pathlib.Path('windows/.smoke/control-input').resolve()/uuid.uuid4().hex;root.mkdir(parents=True)
state=root/'state.txt';ready=root/'ready.json';title='VectorDash input fixture '+uuid.uuid4().hex[:8]
fixture=subprocess.Popen([sys.executable,str(pathlib.Path(__file__).with_name('control_fixture.py')),str(state),str(ready),title,'--tabs'],stdout=subprocess.DEVNULL,stderr=(root/'fixture.log').open('w'))
worker=None
try:
 deadline=time.monotonic()+8
 while not ready.exists() and time.monotonic()<deadline:
  assert fixture.poll() is None,(root/'fixture.log').read_text();time.sleep(.05)
 assert ready.exists()
 info=json.loads(ready.read_text());u=ctypes.WinDLL('user32');u.GetDlgItem.argtypes=[w.HWND,ctypes.c_int];u.GetDlgItem.restype=w.HWND;u.SendMessageW.argtypes=[w.HWND,w.UINT,w.WPARAM,w.LPARAM];u.SendMessageW.restype=ctypes.c_ssize_t
 worker=subprocess.Popen([str(pathlib.Path(a.exe).resolve()),'--control-worker'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,encoding='utf-8')
 u.AllowSetForegroundWindow.argtypes=[w.DWORD];u.AllowSetForegroundWindow(worker.pid)
 u.SetForegroundWindow.argtypes=[w.HWND];u.SetForegroundWindow(info['hwnd']);time.sleep(.2)
 responses=queue.Queue()
 def reader():
  for line in worker.stdout:responses.put(line)
 threading.Thread(target=reader,daemon=True).start()
 def call(action,**args):
  worker.stdin.write(json.dumps({'tool':'computer_use','args':{'action':action,**args},'vision':True,'owner_pid':0})+'\n');worker.stdin.flush();return json.loads(responses.get(timeout=25))
 def good(v):assert 'error' not in v,v;return v
 def text():
  buf=ctypes.create_unicode_buffer(8192);u.SendMessageW(u.GetDlgItem(info['hwnd'],101),0xD,len(buf),ctypes.addressof(buf));return buf.value.replace('\r\n','\n')
 snap=good(call('windows'));wid=next(v['window_id'] for v in snap['windows'] if v['title']==title)
 snap=good(call('inspect',window_id=wid));tabs=u.GetDlgItem(info['hwnd'],120)
 for name,index in [('Singles and EPs',1),('Albums',0),('Singles and EPs',1)]:
  item=next(v for v in snap['elements'] if v.get('name')==name)
  assert 'selected' in item,item
  snap=good(call('click',window_id=wid,snapshot_id=snap['snapshot_id'],element_name=name))
  assert snap['action_method']=='selection',snap.get('action_method')
  assert next(v for v in snap['elements'] if v.get('name')==name)['selected']==True
  assert u.SendMessageW(tabs,0x130b,0,0)==index
 print('PASS actual native tabs: exact-name selection, selected-state verification and three tab switches',flush=True)
 print('Artifacts:',root,flush=True)

finally:
 if worker:worker.terminate();worker.wait()
 fixture.terminate();fixture.wait()
