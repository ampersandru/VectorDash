"""Candidate metadata through actual app/consent/browser helper; localhost fixtures only."""
import argparse,json,os,pathlib,subprocess,threading,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');a=p.parse_args()
root=pathlib.Path('windows/.smoke/candidate-workflow').resolve()/uuid.uuid4().hex;root.mkdir(parents=True);profile=root/'profile';profile.mkdir()
step=0;failures=[]
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def reply(self,data):
  self.send_response(200);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(json.dumps(data).encode())
 def do_GET(self):
  self.send_response(200);self.send_header('Content-Type','text/html');self.end_headers()
  if self.path.startswith('/booking'):
   text='<title>Reservation details</title><p>Fixture only. 7:00 PM for two. No credit card required. Free cancellation until 24 hours before the reservation.</p>'
  else:text='<title>Manzoni Fixture Restaurant</title><p>Fixture only. Pasta $25.</p><a href="/booking/details?rid=20233">Reserve</a>'
  self.wfile.write(text.encode())
 def do_POST(self):
  global step
  try:
   body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
   results=[json.loads(m['content']) for m in body['messages'] if m['role']=='tool'];last=results[-1] if results else {}
   assert 'error' not in last,last
   if step==0:args={'action':'open','url':base+'/manzoni','research':{'candidate':'Manzoni','stage':'details'}}
   elif step==1:
    assert last['research_binding']['candidate']=='Manzoni',last
    url=next(e['href'] for e in last['elements'] if e['name']=='Reserve')
    args={'action':'navigate','url':url,'research':{'candidate':'Manzoni','stage':'availability','note':'Observed fixture booking link'}}
   elif step==2:
    assert last['research_binding']['candidate']=='Manzoni' and 'No credit card required' in last['text'],last
    args={'action':'snapshot','research':{'candidate':'Manzoni','stage':'card_policy','outcome':'observed','note':'Fixture text states no credit card required; not a real booking'}}
   elif step==3:args={'action':'snapshot','research':{'candidate':'Manzoni','stage':'cancellation','outcome':'observed','note':'Fixture text states free cancellation; no booking submitted'}}
   else:
    assert last['research_bound'] and last['research_stage']=='cancellation',last
    self.reply({'choices':[{'message':{'role':'assistant','content':'Candidate workflow verified.'}}]});step+=1;return
   self.reply({'choices':[{'message':{'role':'assistant','tool_calls':[{'id':'call'+str(step),'type':'function','function':{'name':'browser_use','arguments':json.dumps(args)}}]}}]});step+=1
  except Exception as e:failures.append(repr(e));self.reply({'error':'Fixture assertion failed'})
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);base=f'http://127.0.0.1:{server.server_port}';threading.Thread(target=server.serve_forever,daemon=True).start()
settings={'encrypt_index':False,'roots':[],'include_programs':False,'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'language':'en','llm_enabled':True,'llm_url':base+'/v1','llm_model':'fixture','llm_context':16384,'clipboard_enabled':False,'web':{'enabled':False},'browser_use_enabled':True,'control_max_steps':12,'control_browser_path':'C:/Program Files/Google/Chrome/Application/chrome.exe','browser_decision_mode':'Off'}
(profile/'settings.json').write_text(json.dumps(settings),encoding='utf-8')
try:
 subprocess.run([str(pathlib.Path(a.exe).resolve()),'--smoke-tool','AI','--smoke-query','Research a restaurant using only this disposable fixture. Never submit a booking.','--smoke-control-tools','browser','--smoke-control-run','--smoke-control-approval','session','--smoke-seconds','12','--smoke-keep-open','--smoke-report',str(root/'report.json')],env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=35,check=True)
 report=json.loads((root/'report.json').read_text(encoding='utf-8'))
 assert not failures,failures
 assert report['ai_answer']=='Candidate workflow verified.' and not report['ai_busy'],report
 actions=[a for s in report['ai_step_diagnostics'] for a in s['actions']]
 assert all(a['research_bound'] and a['research_candidate_count']==1 for a in actions),actions
 assert [a['research_stage'] for a in actions]==['details','availability','card_policy','cancellation'],actions
 assert report['control']['permission_count']==1,report
 assert not list((profile/'control-temp').glob('session-*')),'Owned helper not cleaned up'
 print('PASS candidate metadata, observed booking-ID binding, staged criteria, diagnostics, consent and cleanup',flush=True)
 print('Artifacts:',root,flush=True)
finally:server.shutdown()
