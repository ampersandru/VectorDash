"""Real Win32/UIA and installed-browser CDP tests; only disposable fixtures are modified."""
import argparse, ctypes, json, os, pathlib, queue, statistics, subprocess, sys, threading, time, uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');p.add_argument('--visible',action='store_true');a=p.parse_args()
exe=pathlib.Path(a.exe).resolve();root=pathlib.Path('windows/.smoke/native-control').resolve()/uuid.uuid4().hex;root.mkdir(parents=True)
state=root/'state.txt';ready=root/'ready.json';title='VectorDash native fixture '+uuid.uuid4().hex[:8]
fixture=subprocess.Popen([sys.executable,str(pathlib.Path(__file__).with_name('control_fixture.py')),str(state),str(ready),title,'--toggle'],stdout=subprocess.DEVNULL,stderr=(root/'fixture-error.log').open('w'))
worker=None;timings={}
html=b"""<!doctype html><meta charset="utf-8"><title>VectorDash browser fixture</title><style>body{font:22px sans-serif;padding:30px}input,button{font:inherit;margin:10px}</style><h1>Disposable browser test</h1><label>Message<input aria-label="Message" id="message"></label><button onclick="document.getElementById('result').textContent='Confirmed: '+document.getElementById('message').value">Apply</button><input type="password" aria-label="Password"><p id="result">Waiting</p>"""
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(html)));self.end_headers();self.wfile.write(html)
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();url=f'http://127.0.0.1:{server.server_port}/'
responses=queue.Queue()
def reader():
 for line in worker.stdout:responses.put(line)
def call(tool,action,**args):
 if tool=='computer_use' and action=='type':args.setdefault('text_mode','replace_all')
 data={'tool':tool,'args':{'action':action,**args},'vision':True,'owner_pid':os.getpid(),'browser_path':'','browser_visible':a.visible,'browser_profile':str(root/'browser')}
 start=time.perf_counter();worker.stdin.write(json.dumps(data)+'\n');worker.stdin.flush()
 try:result=json.loads(responses.get(timeout=25))
 except queue.Empty:raise AssertionError('Native worker timed out')
 timings.setdefault(tool+':'+action+(' (rejected)' if 'error' in result else ''),[]).append((time.perf_counter()-start)*1000);return result
def good(value):assert 'error' not in value,value;return value
try:
 deadline=time.monotonic()+8
 while not ready.exists() and time.monotonic()<deadline:
  assert fixture.poll() is None,(root/'fixture-error.log').read_text();time.sleep(.05)
 assert ready.exists(),'Fixture did not become ready'
 worker=subprocess.Popen([str(exe),'--control-worker'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,encoding='utf-8');threading.Thread(target=reader,daemon=True).start()
 apps=good(call('computer_use','applications',text='Notepad'));assert any(v['name']=='Notepad' for v in apps['applications']);assert all('path' not in v for v in apps['applications'])
 assert 'error' in call('computer_use','launch',app_id='a0000000000000000')
 print('PASS installed-app discovery and unknown launch-ID rejection',flush=True)
 windows=good(call('computer_use','windows'));target=next((v for v in windows['windows'] if v['title']==title),None);assert target,'Fixture missing from window list'
 wid=target['window_id'];snap=good(call('computer_use','inspect',window_id=wid));edit=next(v['id'] for v in snap['elements'] if v.get('value')=='Input' or v.get('name')=='Input')
 switch=next(v for v in snap['elements'] if v.get('automation_id')=='104');assert switch['toggle_state']=='on'
 snap=good(call('computer_use','click',window_id=wid,snapshot_id=snap['snapshot_id'],automation_id='104'));assert next(v for v in snap['elements'] if v.get('automation_id')=='104')['toggle_state']=='off' and state.read_text()=='off'
 snap=good(call('computer_use','click',window_id=wid,snapshot_id=snap['snapshot_id'],automation_id='104'));assert next(v for v in snap['elements'] if v.get('automation_id')=='104')['toggle_state']=='on' and state.read_text()=='on'
 print('PASS native toggle on/off state observations and actual switch changes',flush=True)
 expected='Native control verified: Unicode √'
 good(call('computer_use','type',window_id=wid,snapshot_id=snap['snapshot_id'],element=edit,text=expected))
 stale=call('computer_use','click',window_id=wid,snapshot_id=snap['snapshot_id'],element=edit);assert 'error' in stale
 snap=good(call('computer_use','inspect',window_id=wid));apply=next(v['id'] for v in snap['elements'] if v.get('name')=='Apply');good(call('computer_use','click',window_id=wid,snapshot_id=snap['snapshot_id'],element=apply))
 deadline=time.monotonic()+3
 while not state.exists() and time.monotonic()<deadline:time.sleep(.03)
 assert state.read_text(encoding='utf-8')==expected
 # Chain mutations using only returned observations, never an extra inspect.
 snap=good(call('computer_use','inspect',window_id=wid))
 for expected in ['Alpha','Beta','Gamma','Delta','Epsilon','Final verified chain']:
  edit_node=next(v for v in snap['elements'] if v.get('role')=='edit' or v.get('automation_id')=='101')
  selector={'automation_id':edit_node['automation_id']} if edit_node.get('automation_id') else {'element':edit_node['id']}
  old=snap['snapshot_id'];snap=good(call('computer_use','type',window_id=wid,snapshot_id=old,text=expected,**selector))
  assert snap['performed'] and snap['snapshot_id']!=old
  button=next(v for v in snap['elements'] if v.get('name')=='Apply')
  if button.get('automation_id'):
   rejected=call('computer_use','click',window_id=wid,snapshot_id=snap['snapshot_id'],element=edit_node['id'],automation_id=button['automation_id'])
   assert 'does not match' in rejected.get('error',''),rejected
  snap=good(call('computer_use','click',window_id=wid,snapshot_id=snap['snapshot_id'],element=button['id']))
  assert state.read_text(encoding='utf-8')==expected
 print('PASS twelve chained mutations with fresh returned observations and identity-mismatch rejection',flush=True)
 # Explicit insert/selection modes must preserve surrounding text in any Win32 editor.
 u=ctypes.WinDLL('user32');u.GetDlgItem.argtypes=[ctypes.c_void_p,ctypes.c_int];u.GetDlgItem.restype=ctypes.c_void_p;u.SendMessageW.argtypes=[ctypes.c_void_p,ctypes.c_uint,ctypes.c_size_t,ctypes.c_ssize_t];u.SendMessageW.restype=ctypes.c_ssize_t;u.GetWindowTextW.argtypes=[ctypes.c_void_p,ctypes.c_wchar_p,ctypes.c_int]
 handle=json.loads(ready.read_text())['hwnd'];field=u.GetDlgItem(handle,101)
 def editor_text():
  buffer=ctypes.create_unicode_buffer(4096);u.SendMessageW(field,0xD,len(buffer),ctypes.addressof(buffer));return buffer.value
 text='Before dog and after.'
 snap=good(call('computer_use','type',window_id=wid,snapshot_id=snap['snapshot_id'],automation_id='101',text=text,text_mode='replace_all'))
 for old,new in [('dog','cat'),('cat','flying cow'),('flying cow','dog')]:
  start=text.index(old);u.SendMessageW(field,0xB1,start,start+len(old))
  snap=good(call('computer_use','type',window_id=wid,snapshot_id=snap['snapshot_id'],automation_id='101',text=new,text_mode='replace_selection'))
  text=text.replace(old,new,1);assert editor_text()==text,(editor_text(),text)
 u.SendMessageW(field,0xB1,0,0)
 rejected=call('computer_use','type',window_id=wid,snapshot_id=snap['snapshot_id'],automation_id='101',text='MUST NOT REPLACE',text_mode='replace_selection');assert 'selection' in rejected.get('error','') and editor_text()==text,rejected
 snap=good(call('computer_use','type',window_id=wid,snapshot_id=snap['snapshot_id'],automation_id='101',text='Prefix ',text_mode='insert'));text='Prefix '+text;assert editor_text()==text
 snap=good(call('computer_use','type',window_id=wid,snapshot_id=snap['snapshot_id'],automation_id='101',text=expected,text_mode='replace_all'))
 print('PASS three selection edits, insertion preserving surrounding text, empty-selection rejection and explicit whole-field replacement',flush=True)

 screen=good(call('computer_use','screenshot',window_id=wid));assert screen['image_url'].startswith('data:image/jpeg;base64,') and screen['image_width']>0
 button=next(v for v in screen['elements'] if v.get('name')=='Apply');left,top,right,bottom=screen['window_bounds'];bounds=button['bounds'];x=((bounds[0]+bounds[2])/2-left)*screen['image_width']/(right-left);y=((bounds[1]+bounds[3])/2-top)*screen['image_height']/(bottom-top)
 state.unlink();pixel=call('computer_use','click',window_id=wid,snapshot_id=screen['snapshot_id'],x=x,y=y)
 if 'error' in pixel:
  assert 'Windows blocked foreground input' in pixel['error'],pixel
  assert not state.exists(),'Blocked input unexpectedly clicked the fixture'
  print('PASS physical input: Windows foreground restriction safely rejected the click',flush=True)
 else:
  deadline=time.monotonic()+3
  while not state.exists() and time.monotonic()<deadline:time.sleep(.03)
  assert state.read_text(encoding='utf-8')==expected
  print('PASS physical input: actual coordinate click verified',flush=True)
 print('PASS Windows: enumeration, UIA typing/invoke, actual state verification, stale references and screenshot',flush=True)
 # Minimized apps must remain discoverable; restoration is attempted even if
 # Windows denies the subsequent foreground transfer in an unattended test.
 u=ctypes.WinDLL('user32');u.ShowWindowAsync.argtypes=[ctypes.c_void_p,ctypes.c_int];u.IsIconic.argtypes=[ctypes.c_void_p]
 handle=json.loads(ready.read_text())['hwnd'];u.ShowWindowAsync(handle,6);time.sleep(.15)
 listed=good(call('computer_use','windows'));target=next(v for v in listed['windows'] if v['title']==title);assert target['minimized'];wid=target['window_id']
 assert 'minimized' in call('computer_use','inspect',window_id=wid).get('error','')
 restored=call('computer_use','focus',window_id=wid)
 if 'error' in restored:assert 'Windows blocked foreground input' in restored['error'],restored
 assert not u.IsIconic(handle),'Minimized target was not restored'
 print('PASS minimized window discovery and restoration',flush=True)
 snap=good(call('browser_use','open',url=url));assert snap['url']==url
 if a.visible:
  listed=good(call('computer_use','windows'));b=next(v for v in listed['windows'] if v['title'].startswith('VectorDash browser fixture'))
  assert b['application'].lower() in ['msedge.exe','chrome.exe'],b
  obs=good(call('computer_use','inspect',window_id=b['window_id']));assert obs['elements']
  wid=next(v['window_id'] for v in listed['windows'] if v['title']==title)
  print('PASS existing visible browser discovery and accessibility',flush=True)
 field=next(v['id'] for v in snap['elements'] if v.get('name')=='Message');text=r'Literal text: "quote" \ and <markup>; no script execution'
 good(call('browser_use','type',snapshot_id=snap['snapshot_id'],element=field,text=text))
 assert 'error' in call('browser_use','click',snapshot_id=snap['snapshot_id'],element=field)
 snap=good(call('browser_use','snapshot'));button=next(v['id'] for v in snap['elements'] if v['name']=='Apply');good(call('browser_use','click',snapshot_id=snap['snapshot_id'],element=button))
 snap=good(call('browser_use','snapshot'));assert 'Confirmed: '+text in snap['text'],snap
 password=next(v['id'] for v in snap['elements'] if v.get('password'));assert 'error' in call('browser_use','type',snapshot_id=snap['snapshot_id'],element=password,text='do not type this')
 assert 'error' in call('browser_use','navigate',url='javascript:alert(1)')
 snap=good(call('browser_use','snapshot'));assert snap['url']==url and 'Confirmed: '+text in snap['text']
 screen=good(call('browser_use','screenshot'));assert screen['image_url'].startswith('data:image/jpeg;base64,')
 for _ in range(5):good(call('browser_use','snapshot'));good(call('computer_use','inspect',window_id=wid))
 good(call('browser_use','close'))
 print('PASS browser: isolated launch, navigation, typing/clicks, actual DOM verification, stale references, password/URL rejection, screenshot and close',flush=True)
 summary={key:{'calls':len(value),'median_ms':round(statistics.median(value),1),'max_ms':round(max(value),1)} for key,value in timings.items()}
 (root/'timings.json').write_text(json.dumps(summary,indent=2),encoding='utf-8');print(json.dumps(summary),flush=True)
finally:
 if worker:
  worker.stdin.close()
  try:worker.wait(timeout=4)
  except subprocess.TimeoutExpired:worker.kill();worker.wait()
 fixture.terminate();fixture.wait();server.shutdown()
