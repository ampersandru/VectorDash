"""Opt-in production-LLM desktop regression; API key is read from stdin, never saved.
Uses a disposable profile and records tool messages only in ignored smoke artifacts.
"""
import argparse, base64, json, os, pathlib, subprocess, sys, threading, time, urllib.request, uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
sys.stdout.reconfigure(encoding='utf-8',errors='backslashreplace')
p=argparse.ArgumentParser();p.add_argument('--exe',required=True);p.add_argument('--url',required=True);p.add_argument('--model',required=True);p.add_argument('--case',choices=['fixture','calculator','brave','followup'],default='fixture');p.add_argument('--seconds',type=int,default=180);a=p.parse_args()
secret=sys.stdin.readline().strip();assert secret,'Pass the API key through stdin'
root=pathlib.Path('windows/.smoke/control-live').resolve()/uuid.uuid4().hex;root.mkdir(parents=True)
fixture=None;title='Native editing fixture '+uuid.uuid4().hex[:8];state=root/'state.txt';ready=root/'ready.json'
if a.case in ['fixture','followup']:
 fixture=subprocess.Popen([sys.executable,str(pathlib.Path(__file__).with_name('control_fixture.py')),str(state),str(ready),title,*(['--toggle'] if a.case=='followup' else [])],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 for _ in range(100):
  if ready.exists():break
  time.sleep(.05)
 assert ready.exists()
trace=[]
class Proxy(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):self.reply({'data':[{'id':a.model}]})
 def reply(self,v,status=200):
  raw=json.dumps(v).encode();self.send_response(status);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));body['stream']=False;body.pop('stream_options',None);body['model']=a.model
  for m in body['messages']:
   if isinstance(m.get('content'),list):
    for part in m['content']:
     image=part.get('image_url',{}).get('url','')
     if image.startswith('data:image/'):(root/'last-observation.jpg').write_bytes(base64.b64decode(image.split(',',1)[1]))
   if m.get('role')=='tool' and not any(t.get('tool_call_id')==m.get('tool_call_id') for t in trace):trace.append(m)
  try:
   req=urllib.request.Request(a.url.rstrip('/')+'/chat/completions',data=json.dumps(body).encode(),headers={'Authorization':'Bearer '+secret,'Content-Type':'application/json'})
   with urllib.request.urlopen(req,timeout=115) as r:v=json.load(r)
   message=v['choices'][0]['message'];trace.append(message)
   (root/'trace.json').write_text(json.dumps(trace,ensure_ascii=False,indent=2),encoding='utf-8')
   for c in message.get('tool_calls') or []:print('ACTION',c['function']['arguments'],flush=True)
   if not message.get('tool_calls'):print('ANSWER',message.get('content'),flush=True)
   self.reply(v)
  except Exception as e:
   # Never print request headers or error bodies containing credentials.
   print('API failure:',type(e).__name__,flush=True);self.reply({'error':'Upstream request failed'},502)
server=ThreadingHTTPServer(('127.0.0.1',0),Proxy);threading.Thread(target=server.serve_forever,daemon=True).start()
profile=root/'profile';profile.mkdir()
config={'encrypt_index':False,'roots':[],'include_programs':False,'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'language':'en','default_search_mode':'Names','llm_enabled':True,'llm_vision':True,'llm_url':f'http://127.0.0.1:{server.server_port}/v1','llm_model':a.model,'llm_context':220000,'clipboard_enabled':False,'web':{'enabled':False},'computer_use_enabled':True,'browser_use_enabled':False,'control_max_steps':48}
(profile/'settings.json').write_text(json.dumps(config),encoding='utf-8')
query={'followup':f'Using computer use, in the window titled {title} only, turn the checkbox named Test switch off and verify its state. Do not interact with any other windows. I will ask a follow-up in the same chat.', 'fixture':f'Using computer use, find the window titled {title}. In that window only, type Alpha into its input and click Apply, then type Beta and click Apply, then Gamma and click Apply, then Delta and click Apply, then Epsilon and click Apply, then Final verified chain and click Apply. Verify each confirmed label. Do not interact with any other window. Report the final observed label.', 'calculator':'Open Calculator and calculate 5 * 3 - 2 + 3 * 4 with standard mathematical operator precedence. Verify the actual result displayed in Calculator. Do not claim success unless observed.', 'brave':'Open Brave, go to https://slickdeals.net, and summarize the first deal. Use computer_use for the existing browser. Verify the page and deal from observed content, not prior knowledge.'}[a.case]
print('Artifacts:',root,flush=True)
try:
 subprocess.run([str(pathlib.Path(a.exe).resolve()),'--smoke-report',str(root/'report.json'),'--smoke-screenshot',str(root/'view.png'),'--smoke-seconds',str(a.seconds),'--smoke-keep-open','--smoke-tool','AI','--smoke-query',query,'--smoke-control-tools','computer','--smoke-control-run','--smoke-control-approval','session',*(['--smoke-control-followup','Now turn it back on'] if a.case=='followup' else [])],env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True,timeout=a.seconds+25)
 report=json.loads((root/'report.json').read_text(encoding='utf-8'));print('BUSY',report.get('ai_busy'),'ERROR',report.get('ai_error'),'ANSWER',report.get('ai_answer'),flush=True)
 assert not report.get('ai_busy') and not report.get('ai_error'), 'Live task was unfinished or failed; review observations'
 if a.case=='fixture':assert state.read_text(encoding='utf-8')=='Final verified chain','Live model did not finish the chain'
 if a.case=='followup':assert state.read_text(encoding='utf-8').strip().lower()=='on','Follow-up did not change the actual fixture back to On'
 if a.case=='calculator':
  displays=[e.get('name') for m in trace if m.get('role')=='tool' for e in (json.loads(m['content']).get('elements') or []) if e.get('automation_id')=='CalculatorResults']
  assert displays and displays[-1] in ['Display is 25','Display is 25.'], 'Calculator did not finish with a verified display of 25'
 print('Completed live diagnostic:',a.case,flush=True)
finally:
 server.shutdown()
 if fixture:fixture.terminate();fixture.wait()
 secret=''
