"""Disposable keyboard-input, duplicate-ID and owned-modal regression fixtures."""
import argparse,ctypes,json,pathlib,queue,subprocess,sys,threading,time,uuid
from ctypes import wintypes as w
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');p.add_argument('--require-keyboard',action='store_true');a=p.parse_args()
root=pathlib.Path('windows/.smoke/control-input').resolve()/uuid.uuid4().hex;root.mkdir(parents=True)
state=root/'state.txt';ready=root/'ready.json';title='VectorDash input fixture '+uuid.uuid4().hex[:8]
fixture=subprocess.Popen([sys.executable,str(pathlib.Path(__file__).with_name('control_fixture.py')),str(state),str(ready),title,'--keyboard-input'],stdout=subprocess.DEVNULL,stderr=(root/'fixture.log').open('w'))
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
 focused=call('focus',window_id=wid)
 if 'error' in focused:
  assert not a.require_keyboard and 'Windows blocked foreground input' in focused['error'],focused
  print('SKIP paced keyboard fixture: Windows foreground transfer denied; modal tests continue',flush=True)
  snap=good(call('inspect',window_id=wid))
 else:
  snap=good(focused);edit=next(v for v in snap['elements'] if v.get('automation_id')=='101')
  for i in range(3):
   snap=good(call('key',window_id=wid,snapshot_id=snap['snapshot_id'],element=edit['id'],keys='Ctrl+A'))
   edit=next(v for v in snap['elements'] if v.get('automation_id')=='101')
   expected=('Long Unicode sentence about a flying cow: café, 日本語, 😀. '*5)+'\nA second line with a donut joke.\r\nThird line.'
   snap=good(call('type',window_id=wid,snapshot_id=snap['snapshot_id'],element=edit['id'],text_mode='insert',text=expected))
   assert text()==expected.replace('\r\n','\n'),repr(text())
   edit=next(v for v in snap['elements'] if v.get('automation_id')=='101')
  print('PASS paced keyboard input: long Unicode, surrogate pairs, LF/CRLF, three exact-content runs',flush=True)
 button=next(v for v in snap['elements'] if v.get('name')=='Open dialog')
 snap=good(call('click',window_id=wid,snapshot_id=snap['snapshot_id'],element=button['id']))
 assert snap['title']=='VectorDash modal fixture' and snap['modal_dialog'],snap
 snap=good(call('screenshot',window_id=wid));bounds=snap['window_bounds'];ratio=(bounds[2]-bounds[0])/(bounds[3]-bounds[1]);assert snap['title']=='VectorDash modal fixture' and snap.get('image_url') and abs(snap['image_width']/snap['image_height']-ratio)<.02,{k:v for k,v in snap.items() if k not in ('image_url','elements')}
 saves=[v for v in snap['elements'] if v.get('name')=='Save'];assert len(saves)==2 and all('automation_id' not in v for v in saves),saves
 rejected=call('click',window_id=wid,snapshot_id=snap['snapshot_id'],automation_id='1');assert 'ambiguous' in rejected.get('error',''),rejected
 snap=good(call('click',window_id=wid,snapshot_id=snap['snapshot_id'],element=saves[1]['id'],automation_id='1'))
 assert state.read_text(encoding='utf-8')=='modal second clicked' and snap['title']==title,snap
 print('PASS same-application modal inspect/screenshot/action/return and duplicate automation-ID selection',flush=True)
 print('Artifacts:',root,flush=True)
finally:
 if worker:worker.terminate();worker.wait()
 fixture.terminate();fixture.wait()
