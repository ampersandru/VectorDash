"""Isolated real-client checks for step diagnostics; localhost mock only."""
import argparse,json,os,pathlib,subprocess,threading,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/release/vectordash.exe');a=p.parse_args()
root=(pathlib.Path('windows/.smoke/ai-diagnostics')/uuid.uuid4().hex).resolve();root.mkdir(parents=True)
case='json-cached';requests=[]
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));requests.append(body)
  if case=='http-error':self.send_response(503);self.end_headers();self.wfile.write(b'PRIVATE_SERVER_ERROR');return
  self.send_response(200)
  stats={'usage':{'prompt_tokens':30000,'completion_tokens':100,'prompt_tokens_details':{'cached_tokens':975 if case=='json-cached' else 0}},'timings':{'prompt_n':29025 if case=='json-cached' else 30000,'prompt_ms':16000,'predicted_ms':1000}}
  if case=='missing':stats={}
  if case=='stream-zero':
   self.send_header('Content-Type','text/event-stream');self.end_headers()
   for value in [{'choices':[{'delta':{'role':'assistant','content':''}}]},{'choices':[{'delta':{'content':'Fixture response.'}}]},{'choices':[],**stats}]:self.wfile.write(('data: '+json.dumps(value)+'\n\n').encode())
   self.wfile.write(b'data: [DONE]\n\n')
  else:
   raw=json.dumps({'choices':[{'message':{'role':'assistant','content':'Fixture response.'}}],**stats}).encode();self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
try:
 for case in ['json-cached','stream-zero','missing','http-error']:
  folder=root/case;profile=folder/'profile';profile.mkdir(parents=True);requests.clear()
  config={'encrypt_index':False,'roots':[],'include_programs':False,'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'language':'en','default_search_mode':'Names','llm_enabled':True,'llm_url':f'http://127.0.0.1:{server.server_port}/v1','llm_key':'PRIVATE_TEST_KEY','llm_model':'fixture','llm_context':8192,'llm_response':'Full','clipboard_enabled':False,'web':{'enabled':False}}
  (profile/'settings.json').write_text(json.dumps(config),encoding='utf-8')
  subprocess.run([str(pathlib.Path(a.exe).resolve()),'--smoke-tool','AI','--smoke-followup','PRIVATE_TEST_PROMPT','--smoke-diagnostics-open','--smoke-seconds','7','--smoke-keep-open','--smoke-report',str(folder/'report.json'),'--smoke-screenshot',str(folder/'view.png')],env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=30,check=True)
  report=json.loads((folder/'report.json').read_text());rows=report['ai_step_diagnostics'];assert len(rows)==1,report
  row=rows[0];assert row['step']==1 and row['model_ms']>=0 and row['messages']>=1 and row['text_bytes']>0,row
  log=(profile/'vectordash.log').read_text(encoding='utf-8');lines=[line for line in log.splitlines() if 'AI step diagnostics job=' in line];assert len(lines)==1,lines
  assert all(secret not in lines[0] for secret in ['PRIVATE_TEST_KEY','PRIVATE_TEST_PROMPT','PRIVATE_SERVER_ERROR','Fixture response.']),lines
  assert requests[0]['stream_options']['include_usage'] is True
  if case=='http-error':assert row['status']=='error' and row['http_status']==503 and row['input_tokens'] is None,row
  else:
   assert report['ai_answer']=='Fixture response.' and row['status']=='answer',report
   assert row['first_response_byte_ms'] is not None,row
   if case=='missing':assert row['input_tokens'] is None and row['cached_tokens'] is None and row['output_tokens'] is None,row
   else:assert row['input_tokens']==30000 and row['output_tokens']==100 and row['cached_tokens']==(975 if case=='json-cached' else 0),row
   assert (row['first_token_ms'] is not None)==(case=='stream-zero'),row
  print(case,'PASS',flush=True)
 print('PASS: four diagnostic GUI/log/privacy cases; artifacts:',root,flush=True)
finally:server.shutdown()
