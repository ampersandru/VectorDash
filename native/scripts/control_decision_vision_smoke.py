"""Real consent/worker screenshot routing with a text-only chat model and mock vision decision API.
Only disposable native and browser fixtures are touched.
"""
import argparse,base64,io,json,os,pathlib,subprocess,sys,threading,time,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from PIL import Image
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');a=p.parse_args()
root=pathlib.Path('windows/.smoke/decision-vision-chat').resolve()/uuid.uuid4().hex;root.mkdir(parents=True)
state=root/'state.txt';ready=root/'ready.json';title='VectorDash vision fixture '+uuid.uuid4().hex[:8]
fixture=subprocess.Popen([sys.executable,str(pathlib.Path(__file__).with_name('control_fixture.py')),str(state),str(ready),title],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
case='';steps=0;real_images=0;failures=[]
html=b'<meta charset="utf-8"><title>Vision fixture</title><label>Message<input aria-label="Message"></label><button onclick="document.querySelector(\'p\').textContent=\'Confirmed: \'+document.querySelector(\'input\').value">Apply</button><p>Waiting</p>'
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def reply(self,data):
  data=json.dumps(data).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
 def do_GET(self):
  if self.path.endswith('/models'):self.reply({'data':[{'id':'vision-fixture','architecture':{'input_modalities':['text','image'],'output_modalities':['decisions']}}]});return
  self.send_response(200);self.send_header('Content-Type','text/html');self.end_headers();self.wfile.write(html)
 def do_POST(self):
  global steps,real_images
  try:
   body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
   if self.path.startswith('/decision/'):
    criteria=body['questions']['next']['criteria'];image=Image.open(io.BytesIO(base64.b64decode(body['images'][0].split(',',1)[1]))).convert('RGB')
    assert image.width<=448 and image.height<=448 and image.width%32==0 and image.height%32==0
    if 'red' in criteria:chosen='red' if image.getpixel((0,0))[0]>image.getpixel((0,0))[2] else 'blue'
    else:
     real_images+=1
     chosen=next((k for k,v in criteria.items() if 'Apply' in v),None) if body['state']['action']=='click' else next(k for k in criteria if k.startswith('target_'))
     assert chosen,criteria
    self.reply({'answers':{'next':{'choice':chosen,'probabilities':{k:.99 if k==chosen else .01/(len(criteria)-1) for k in criteria}}}});return
   assert not any(isinstance(m.get('content'),list) and any(v.get('type')=='image_url' for v in m['content']) for m in body['messages']),'Decision screenshot leaked to text-only chat model'
   results=[json.loads(m['content']) for m in body['messages'] if m['role']=='tool'];last=results[-1] if results else {};assert 'error' not in last,last
   if steps==0:args={'action':'windows'} if case=='computer' else {'action':'open','url':base+'/fixture'}
   elif steps==1:
    args={'action':'inspect','window_id':next(v['window_id'] for v in last['windows'] if v['title']==title)} if case=='computer' else {'action':'snapshot'}
   elif steps==2:
    args={'action':'type','text':'Vision fixture','element':next(v['id'] for v in last['elements'] if v.get('control_type')==50004 or v.get('role') in ['edit','textbox'] or v.get('name')=='Message'),'snapshot_id':last['snapshot_id']}
    if case=='computer':args.update(window_id=last['window_id'],text_mode='replace_all')
   elif steps==3:
    assert last['decision']['route']=='automatic_plan' and last['decision']['executed']==1,last
    args={'action':'decide','plan':[{'goal':'Apply','action':'click','verify_text':'Vision fixture'}]}
    if case=='computer':args['window_id']=last['window_id']
   else:
    assert last['decision']['status']=='verified',last
    assert last['decision']['executed']==1,last
    assert 'image_url' not in last
    if case=='computer':assert state.read_text(encoding='utf-8')=='Vision fixture'
    else:assert 'Confirmed: Vision fixture' in last['text']
    self.reply({'choices':[{'message':{'role':'assistant','content':'Vision routing verified.'}}]});steps+=1;return
   self.reply({'choices':[{'message':{'role':'assistant','tool_calls':[{'id':'call'+str(steps),'type':'function','function':{'name':case+'_use','arguments':json.dumps(args)}}]}}]});steps+=1
  except Exception as error:
   failures.append(repr(error));self.reply({'error':'Fixture assertion failed'})
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);base=f'http://127.0.0.1:{server.server_port}';threading.Thread(target=server.serve_forever,daemon=True).start()
try:
 deadline=time.monotonic()+8
 while not ready.exists() and time.monotonic()<deadline:time.sleep(.05)
 assert ready.exists()
 for case in ['computer','browser']:
  steps=0;real_images=0;failures.clear();folder=root/case;profile=folder/'profile';profile.mkdir(parents=True)
  settings={'encrypt_index':False,'roots':[],'include_programs':False,'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'language':'en','llm_enabled':True,'llm_vision':False,'llm_url':base+'/v1','llm_model':'text-only-fixture','llm_context':16384,'clipboard_enabled':False,'web':{'enabled':False},'computer_use_enabled':True,'browser_use_enabled':True,'control_max_steps':12,'control_browser_path':'C:/Program Files/Google/Chrome/Application/chrome.exe','computer_decision_mode':'Execute','browser_decision_mode':'Execute','decision_vision':True,'decision_url':base+'/decision'}
  (profile/'settings.json').write_text(json.dumps(settings),encoding='utf-8')
  command=[str(pathlib.Path(a.exe).resolve()),'--smoke-tool','AI','--smoke-query','Use only the disposable fixture.','--smoke-control-tools',case,'--smoke-control-run','--smoke-control-approval','session','--smoke-seconds','12','--smoke-keep-open','--smoke-report',str(folder/'report.json')]
  subprocess.run(command,env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=30,check=True)
  report=json.loads((folder/'report.json').read_text(encoding='utf-8'))
  assert not failures,failures
  assert report['ai_answer']=='Vision routing verified.' and not report['ai_busy'],report
  assert real_images==2,(real_images,report)
  actions=[a for step in report['ai_step_diagnostics'] for a in step['actions']]
  assert not any(a['decision_route']=='blocked' for a in actions),actions
  assert any(a['decision_route']=='automatic_plan' and a['decision_calls']==1 and a['decision_executed']==1 and a['decision_probability']==.99 for a in actions),actions
  assert any(a['decision_route']=='local_plan' and a['decision_calls']==1 and a['decision_executed']==1 and a['decision_vision_images']==1 and a['decision_backend']=='remote' and a['decision_inference_ms']>=0 for a in actions),actions
  assert report['control']['permission_count']==1,report
  assert not list((profile/'control-temp').glob('session-*')),'Owned helper was not cleaned up'
  print('PASS',case,'real screenshot routing, two guarded actions, consent, and text-only chat privacy',flush=True)
 print('Artifacts:',root,flush=True)
finally:
 server.shutdown();fixture.terminate();fixture.wait()
