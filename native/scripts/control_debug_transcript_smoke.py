"""Two AI turns append to one opt-in file using a localhost model and read-only window observations."""
import argparse,json,os,pathlib,subprocess,threading,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');a=p.parse_args()
root=pathlib.Path('windows/.smoke/debug-transcript').resolve()/uuid.uuid4().hex;root.mkdir(parents=True);profile=root/'profile';profile.mkdir()
count=0
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  text=b'<title>Debug transcript fixture</title><p>Disposable local fixture</p>';self.send_response(200);self.send_header('Content-Length',str(len(text)));self.end_headers();self.wfile.write(text)
 def do_POST(self):
  global count
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  if count%2==0:
   args={'action':'windows'}
   message={'role':'assistant','tool_calls':[{'id':'call'+str(count),'type':'function','function':{'name':'computer_use','arguments':json.dumps(args)}}]}
  else:message={'role':'assistant','content':'First verified.' if count==1 else 'Follow-up verified.'}
  count+=1;data=json.dumps({'choices':[{'message':message}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);base=f'http://127.0.0.1:{server.server_port}';threading.Thread(target=server.serve_forever,daemon=True).start()
settings={'encrypt_index':False,'roots':[],'include_programs':False,'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'language':'en','clipboard_enabled':False,'llm_enabled':True,'llm_url':base+'/v1','llm_key':'debug-fixture-api-secret-must-not-log','llm_model':'fixture','llm_context':16384,'computer_use_enabled':True,'control_browser_path':'C:/Program Files/Google/Chrome/Application/chrome.exe','control_max_steps':8,'control_debug':True,'browser_decision_mode':'Off','web':{'enabled':False}}
(profile/'settings.json').write_text(json.dumps(settings),encoding='utf-8')
try:
 subprocess.run([str(pathlib.Path(a.exe).resolve()),'--smoke-tool','AI','--smoke-query','Inspect the disposable fixture.','--smoke-followup','Inspect that fixture again.','--smoke-control-tools','computer','--smoke-control-run','--smoke-control-approval','session','--smoke-seconds','12','--smoke-keep-open','--smoke-report',str(root/'report.json')],env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=35,check=True)
 report=json.loads((root/'report.json').read_text(encoding='utf-8'));assert report['ai_answer']=='Follow-up verified.' and count==4,(report,count)
 files=list((profile/'control-debug').glob('*.jsonl'));assert len(files)==1,files
 text=files[0].read_text(encoding='utf-8');rows=[json.loads(line)for line in text.splitlines()]
 requests=[r for r in rows if r['event']=='request'];responses=[r for r in rows if r['event']=='response']
 assert [r['prompt']for r in requests]==['Inspect the disposable fixture.','Inspect that fixture again.'],requests
 assert [r['text']for r in responses]==['First verified.','Follow-up verified.'],responses
 assert len({r['job']for r in requests})==2
 assert 'debug-fixture-api-secret-must-not-log' not in text and 'data:image/' not in text
 assert all(any(r['event']=='tool_result' and r['job']==request['job'] for r in rows)for request in requests)
 print('PASS two chat turns, prompts/responses and actual steps in one file; API setting excluded',flush=True);print('Artifacts:',root,flush=True)
finally:server.shutdown()
