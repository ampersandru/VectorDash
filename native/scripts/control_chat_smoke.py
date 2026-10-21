"""Remote-API chat wiring and real consent modal tests, using only local fixtures."""
import ctypes, argparse, json, os, pathlib, subprocess, threading, time, uuid, sys
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');p.add_argument('--case');a=p.parse_args();exe=str(pathlib.Path(a.exe).resolve())
root=pathlib.Path('windows/.smoke/control-chat').resolve()/uuid.uuid4().hex;root.mkdir(parents=True)
requests=[];case='';checks=[]
fixture_title='VectorDash chat fixture '+uuid.uuid4().hex[:8]
fixture_state=root/'fixture-state.txt';fixture_ready=root/'fixture-ready.json'
fixture=subprocess.Popen([sys.executable,str(pathlib.Path(__file__).with_name('control_fixture.py')),str(fixture_state),str(fixture_ready),fixture_title],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
deadline=time.monotonic()+8
while not fixture_ready.exists() and time.monotonic()<deadline:time.sleep(.05)
assert fixture_ready.exists(),'Fixture did not start'
page=b'<meta charset="utf-8"><title>Control fixture</title><h1>Local fixture only</h1><input aria-label="Message" id="message"><button onclick="document.getElementById(\'result\').textContent=\'Confirmed: \'+document.getElementById(\'message\').value">Apply</button><p id="result">Waiting</p>'
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def send(self,value):
  data=json.dumps(value).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
 def do_GET(self):
  if self.path.startswith('/v1/models'):self.send({'data':[{'id':'remote-control-fixture'}]});return
  self.send_response(200);self.send_header('Content-Type','text/html');self.send_header('Content-Length',str(len(page)));self.end_headers();self.wfile.write(page)
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers.get('Content-Length',0))));requests.append(body);step=len(requests)-1
  def call(name,args):self.send({'choices':[{'message':{'role':'assistant','content':None,'tool_calls':[{'id':f'call{step}','type':'function','function':{'name':name,'arguments':json.dumps(args)}}]}}]})
  def answer(text):self.send({'choices':[{'message':{'role':'assistant','content':text}}]})
  if not body.get('tools'):
   if case=='calculator':
    observations=[json.loads(m['content'] if m['role']=='tool' else m['content'].split('\n',1)[1]) for m in body['messages'] if m['role']=='tool' or isinstance(m.get('content'),str) and m['content'].startswith('Retrieved search data (')]
    last=next(v for v in reversed(observations) if v.get('elements'))
    assert not self.server.calc_buttons and any(v['name']=='Display is 15' for v in last['elements']),last
    answer('Calculator displayed 15.');return
   answer('Permission denied; no actions performed.' if case=='deny' else 'Tools disabled.');return
  assert body.get('parallel_tool_calls') is False
  if case.startswith('followup-'):
   messages=body['messages'];latest=max(i for i,m in enumerate(messages) if m['role']=='user' and isinstance(m.get('content'),str));following=messages[latest]['content']=='Now restore the fixture'
   results=[json.loads(m['content']) for m in messages[latest+1:] if m['role']=='tool'];last=results[-1] if results else {};assert 'error' not in last,last
   if not results:
    assert body['tool_choice']=='required'
    if following:
     assert any(m['role']=='tool' and 'Off' in m.get('content','') for m in messages[:latest]),'Verified previous tool context missing'
     if case=='followup-no-tool':answer('The fixture is now On.');return
    call('computer_use',{'action':'windows'});return
   if len(results)==1:
    self.server.fixture_wid=next(v['window_id'] for v in last['windows'] if v['title']==fixture_title);call('computer_use',{'action':'inspect','window_id':self.server.fixture_wid});return
   if len(results)==2:
    field=next(v for v in last['elements'] if v.get('automation_id')=='101');call('computer_use',{'action':'type','window_id':self.server.fixture_wid,'snapshot_id':last['snapshot_id'],'automation_id':field['automation_id'],'text':'On' if following else 'Off'});return
   if len(results)==3:
    button=next(v for v in last['elements'] if v['name']=='Apply');call('computer_use',{'action':'click','window_id':self.server.fixture_wid,'snapshot_id':last['snapshot_id'],'element':button['id']});return
   assert fixture_state.read_text(encoding='utf-8')==('On' if following else 'Off');answer('Verified On.' if following else 'Verified Off.');return
  if case=='calculator':
   time.sleep(.2) # A real remote model also gives packaged apps time to finish layout.
   results=[json.loads(m['content']) for m in body['messages'] if m['role']=='tool'];last=results[-1] if results else {}
   if 'error' in last:
    assert any(v in last['error'] for v in ['Revalidating','Element no longer exists','Element changed','Window moved','identity changed']),last
    call('computer_use',{'action':'inspect','window_id':self.server.calc_wid});return
   if not results:
    self.server.calc_buttons=['Clear','Five','Multiply by','Three','Equals'];call('computer_use',{'action':'launch','application':'Calculator'});return
   if last.get('launch_requested'):
    assert last['window_present'],last;self.server.calc_wid=last['window_id'];call('computer_use',{'action':'inspect','window_id':self.server.calc_wid});return
   wid=self.server.calc_wid
   if last.get('performed'):
    self.server.calc_buttons.pop(0);call('computer_use',{'action':'inspect','window_id':wid});return
   if self.server.calc_buttons:
    name=self.server.calc_buttons[0];node=next((v for v in last['elements'] if v['name']==name),None)
    if node is None:
     print('Calculator controls not ready:',name,[v['name'] for v in last.get('elements',[])],flush=True);call('computer_use',{'action':'inspect','window_id':wid});return
    call('computer_use',{'action':'click','window_id':wid,'snapshot_id':last['snapshot_id'],'element':node['id']});return
   assert any(v['name']=='Display is 15' for v in last['elements']),last
   answer('Calculator displayed 15.');return
  if case=='discovery':
   if step==0:call('computer_use',{'action':'applications','text':'Notepad'});return
   last=json.loads(next(m['content'] for m in reversed(body['messages']) if m['role']=='tool'));assert any(v['name']=='Notepad' for v in last['applications']),last
   answer('Installed application catalogue verified without launching.');return
  if case=='applications':
   results=[json.loads(m['content']) for m in body['messages'] if m['role']=='tool'];last=results[-1] if results else {};assert 'error' not in last,last.get('error')
   if step==0:call('computer_use',{'action':'applications','text':'Notepad'});return
   if step==1:
    app=next(v for v in last['applications'] if v['name']=='Notepad');self.server.launch_id=app['app_id'];call('computer_use',{'action':'launch','app_id':app['app_id']});return
   if step==2:
    assert last['launch_requested'] and last['application']=='Notepad';self.server.launch_deadline=time.monotonic()+6;call('computer_use',{'action':'windows'});return
   observed=any(v['application'].lower()=='notepad.exe' or 'notepad' in v['title'].lower() for v in last['windows'])
   if not observed and time.monotonic()<self.server.launch_deadline:
    time.sleep(.5);call('computer_use',{'action':'windows'});return
   assert observed,'Windows accepted launch but no Notepad window appeared within the observation deadline'
   answer('Installed Notepad application opened and observed.');return
  if case=='desktop':
   results=[json.loads(m['content']) for m in body['messages'] if m['role']=='tool'];last=results[-1] if results else {};assert 'error' not in last,last.get('error')
   if step==0:call('computer_use',{'action':'windows'});return
   if step==1:
    target=next(v for v in last['windows'] if v['title']==fixture_title);self.server.fixture_wid=target['window_id'];call('computer_use',{'action':'inspect','window_id':self.server.fixture_wid});return
   wid=self.server.fixture_wid
   if step==2:
    edit=next(v['id'] for v in last['elements'] if v.get('automation_id')=='101');call('computer_use',{'action':'type','window_id':wid,'snapshot_id':last['snapshot_id'],'element':edit,'text':'Typed through remote desktop chat'});return
   if step==3:call('computer_use',{'action':'screenshot','window_id':wid});return
   if step==4:
    button=next(v for v in last['elements'] if v['name']=='Apply');call('computer_use',{'action':'click','window_id':wid,'snapshot_id':last['snapshot_id'],'element':button['id']});return
   if step==5:call('computer_use',{'action':'inspect','window_id':wid});return
   assert fixture_state.read_text(encoding='utf-8')=='Typed through remote desktop chat';answer('Desktop fixture verified.');return
  if case=='browser':
   results=[json.loads(m['content']) for m in body['messages'] if m['role']=='tool'];last=results[-1] if results else {}
   assert 'error' not in last,last.get('error')
   if step==0:call('browser_use',{'action':'open','url':base+'/fixture'});return
   if step==1:call('browser_use',{'action':'type','snapshot_id':last['snapshot_id'],'element':next(v['id'] for v in last['elements'] if v['name']=='Message'),'text':'Typed through remote chat'});return
   if step in [2,4]:call('browser_use',{'action':'snapshot'});return
   if step==3:call('browser_use',{'action':'click','snapshot_id':last['snapshot_id'],'element':next(v['id'] for v in last['elements'] if v['name']=='Apply')});return
   if step==5:
    assert 'Confirmed: Typed through remote chat' in last['text'];checks.append('actual DOM verified');call('browser_use',{'action':'screenshot'});return
   images=[m for m in body['messages'] if isinstance(m.get('content'),list) and any(v.get('type')=='image_url' for v in m['content'])]
   assert len(images)==1
   assert any(m.get('content')=='Use only the local disposable fixture.' for m in body['messages'])
   if step==6:call('browser_use',{'action':'screenshot'});return
   checks.append('single current screenshot and original task retained');answer('Browser fixture verified.');return
  if step<2:call('computer_use',{'action':'windows'});return
  answer('Window inspection complete.')
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);base=f'http://127.0.0.1:{server.server_port}';threading.Thread(target=server.serve_forever,daemon=True).start()
def calculator_handles():
 from ctypes import wintypes as w
 u=ctypes.WinDLL('user32');u.GetWindowTextW.argtypes=[w.HWND,w.LPWSTR,ctypes.c_int];out=set()
 @ctypes.WINFUNCTYPE(w.BOOL,w.HWND,w.LPARAM)
 def collect(hwnd,param):
  title=ctypes.create_unicode_buffer(256);u.GetWindowTextW(hwnd,title,len(title))
  if title.value=='Calculator' or 'notepad' in title.value.lower():out.add(hwnd)
  return True
 u.EnumWindows(collect,0);return out

def run(name,settings,flags=(),seconds=6):
 global case
 case=name;requests.clear();folder=root/name;profile=folder/'profile';profile.mkdir(parents=True)
 config={'encrypt_index':False,'roots':[],'include_programs':False,'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'language':'en','default_search_mode':'Names','llm_enabled':True,'llm_vision':True,'llm_url':base+'/v1','llm_model':'remote-control-fixture','llm_context':16384,'clipboard_enabled':False,'web':{'enabled':False},'computer_use_enabled':True,'browser_use_enabled':True,'control_max_steps':12}
 config.update(settings);(profile/'settings.json').write_text(json.dumps(config),encoding='utf-8')
 command=[exe,'--smoke-report',str(folder/'report.json'),'--smoke-screenshot',str(folder/'view.png'),'--smoke-seconds',str(seconds),'--smoke-keep-open','--smoke-tool','AI','--smoke-query','Use only the local disposable fixture.','--smoke-control-tools','browser' if name=='browser' else 'computer','--smoke-control-run',*flags]
 before=calculator_handles() if name in ['calculator','applications'] else set()
 try:subprocess.run(command,env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True,timeout=seconds+15)
 finally:
  if name in ['calculator','applications']:
   from ctypes import wintypes as w
   u=ctypes.WinDLL('user32');u.PostMessageW.argtypes=[w.HWND,w.UINT,w.WPARAM,w.LPARAM]
   for hwnd in calculator_handles()-before:u.PostMessageW(hwnd,0x10,0,0)

 assert not list((profile/'control-temp').glob('session-*')),'Temporary browser/helper profile was not cleaned up'
 report=json.loads((folder/'report.json').read_text(encoding='utf-8'));print('PASS',name,flush=True);return report
try:
 cases=[('followup-session',{},('--smoke-control-approval','session','--smoke-control-followup','Now restore the fixture'),10),('followup-once',{},('--smoke-control-approval','once','--smoke-control-followup','Now restore the fixture'),10),('followup-no-tool',{},('--smoke-control-approval','session','--smoke-control-followup','Now restore the fixture'),10),('disabled',{'computer_use_enabled':False,'browser_use_enabled':False},(),6),('deny',{},('--smoke-control-approval','deny'),6),('once',{},('--smoke-control-approval','once'),6),('session',{},('--smoke-control-approval','session'),6),('pending',{},('--smoke-quick',),6),('new-chat',{},('--smoke-control-new-chat',),6),('browser',{},('--smoke-control-approval','session'),12),('desktop',{},('--smoke-control-approval','session'),12),('discovery',{},('--smoke-control-approval','session'),6),('applications',{'control_max_steps':20},('--smoke-control-approval','session'),14),('calculator',{},('--smoke-control-approval','session'),16)]
 count=0
 for name,settings,flags,seconds in cases:
  if a.case and a.case!=name:continue
  if name=='calculator' and not a.case:continue # Optional packaged-app accessibility diagnostic.
  r=run(name,settings,flags,seconds);count+=1
  if name.startswith('followup-'):
   assert not r['ai_busy'],r
   if name=='followup-no-tool':assert (not r['ai_error'] or 'without using' in r['ai_error']) and requests[-1].get('tool_choice')=='required' and not r['ai_answer'] and fixture_state.read_text(encoding='utf-8')=='Off',r
   else:assert r['ai_answer']=='Verified On.' and r['control']['permission_count']==(8 if name=='followup-once' else 1) and fixture_state.read_text(encoding='utf-8')=='On',r
   if name!='followup-no-tool':
    rows=r['ai_step_diagnostics'];assert len(rows)==5 and rows[0]['step']==1 and rows[-1]['status']=='answer',rows
    assert sum(v['tool_calls'] for v in rows)==4 and all(v['input_tokens'] is None and v['cached_tokens'] is None for v in rows),rows
  elif name=='disabled':assert not r['control']['computer_toggle'] and r['control']['permission_count']==0 and r['ai_answer']=='Tools disabled.',r
  elif name=='deny':assert r['control']['permission_count']==1 and not r['ai_busy'] and r['ai_answer']=='Permission denied; no actions performed.',r
  elif name=='once':assert r['control']['permission_count']==2 and r['ai_answer']=='Window inspection complete.',r
  elif name=='session':assert r['control']['permission_count']==1 and r['ai_answer']=='Window inspection complete.',r
  elif name=='pending':assert r['control']['pending'] and r['ai_busy'] and r['control']['permission_count']==1,r
  elif name=='new-chat':assert not r['control']['pending'] and not r['control']['computer_toggle'] and r['control']['permission_count']==0 and not r['ai_busy'],r
  elif name=='discovery':assert r['ai_answer']=='Installed application catalogue verified without launching.' and r['control']['permission_count']==1 and not r['ai_busy'],r
  elif name=='applications':assert r['ai_answer']=='Installed Notepad application opened and observed.' and r['control']['permission_count']==1 and not r['ai_busy'],r
  elif name=='calculator':assert r['ai_answer']=='Calculator displayed 15.' and r['control']['permission_count']==1 and not r['ai_busy'],r
  elif name=='desktop':assert r['ai_answer']=='Desktop fixture verified.' and r['control']['permission_count']==1 and r['window_visible'] and not r['ai_busy'],r
  elif name=='browser':assert r['ai_answer']=='Browser fixture verified.' and r['control']['permission_count']==1 and not r['ai_busy'] and len(checks)==2,r
 print(f'PASS: {count} consent/remote-chat cases',flush=True)
 print('Artifacts:',root,flush=True)
finally:
 (root/'requests.json').write_text(json.dumps(requests,ensure_ascii=False,indent=2),encoding='utf-8')
 server.shutdown();fixture.terminate();fixture.wait()
