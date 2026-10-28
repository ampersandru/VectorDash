"""Completion-guard regression with a disposable native editor and mock model."""
import argparse,json,os,pathlib,subprocess,sys,threading,time,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');a=p.parse_args()
root=pathlib.Path('windows/.smoke/control-completion').resolve()/uuid.uuid4().hex;root.mkdir(parents=True)
ready=root/'ready.json';state=root/'state.txt';title='VectorDash completion fixture '+uuid.uuid4().hex[:8]
fixture=subprocess.Popen([sys.executable,str(pathlib.Path(__file__).with_name('control_fixture.py')),str(state),str(ready),title],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
step=0;case='';failures=[]
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_POST(self):
  global step
  try:
   body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));results=[json.loads(m['content'])for m in body['messages']if m['role']=='tool'];last=results[-1]if results else{}
   if step==0:args={'action':'windows'}
   elif step==1 or case=='refusal':
    args=None;message={'role':'assistant','content':'Done saved successfully. All requested changes are complete.'}
   elif step==2:
    assert any('VectorDash completion check' in str(m.get('content',''))for m in body['messages'])
    wid=next(w['window_id']for w in results[0]['windows']if w['title']==title);args={'action':'inspect','window_id':wid}
   elif step==3:
    target=next(r for r in last['elements']if r.get('automation_id')=='101');args={'action':'type','window_id':last['window_id'],'snapshot_id':last['snapshot_id'],'element':target['id'],'text_mode':'replace_all','text':'Alpha'}
   else:
    assert last.get('performed') and any(r.get('value')=='Alpha'for r in last['elements']),last
    args=None;message={'role':'assistant','content':'Typed Alpha; the editor value is verified.'}
   if args is not None:message={'role':'assistant','tool_calls':[{'id':'c'+str(step),'type':'function','function':{'name':'computer_use','arguments':json.dumps(args)}}]}
   step+=1;data=json.dumps({'choices':[{'message':message}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
  except Exception as e:failures.append(type(e).__name__);self.send_response(500);self.end_headers()
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
try:
 for _ in range(100):
  if ready.exists():break
  time.sleep(.05)
 for case in ['recovery','refusal']:
  step=0;failures.clear();folder=root/case;folder.mkdir();profile=folder/'profile';profile.mkdir()
  settings={'encrypt_index':False,'roots':[],'include_programs':False,'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'language':'en','clipboard_enabled':False,'llm_enabled':True,'llm_url':f'http://127.0.0.1:{server.server_port}/v1','llm_model':'fixture','llm_context':16384,'computer_use_enabled':True,'computer_decision_mode':'Off','control_debug':True,'control_max_steps':8,'web':{'enabled':False}}
  (profile/'settings.json').write_text(json.dumps(settings),encoding='utf-8')
  subprocess.run([str(pathlib.Path(a.exe).resolve()),'--smoke-tool','AI','--smoke-query',f'Type Alpha into the Input editor of {title} using actual computer tools.','--smoke-control-tools','computer','--smoke-control-run','--smoke-control-approval','session','--smoke-seconds','10','--smoke-keep-open','--smoke-report',str(folder/'report.json')],env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=30,check=True)
  report=json.loads((folder/'report.json').read_text(encoding='utf-8'));assert not failures,failures;assert 'Done saved successfully' not in report['ai_answer'],report['ai_answer']
  assert sum(s['status']=='completion_retry'for s in report['ai_step_diagnostics'])==1
  if case=='recovery':assert report['ai_answer']=='Typed Alpha; the editor value is verified.' and step==5,(report,step)
  else:assert 'withheld an unverified' in report['ai_answer'] and step==3,(report,step)
  logs=list((profile/'control-debug').glob('*.jsonl'));rows=[json.loads(line)for line in logs[0].read_text(encoding='utf-8').splitlines()]
  assert any(r['event']=='tool_exchange'for r in rows)
  print('PASS',case,'one bounded correction; fabricated draft withheld; actual tool exchanges exported',flush=True)
 print('Artifacts:',root,flush=True)
finally:server.shutdown();fixture.terminate();fixture.wait()
