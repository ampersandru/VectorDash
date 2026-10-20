"""AI response rendering GUI fixtures; isolated profile and localhost-only mock LLM."""
import argparse,json,os,pathlib,subprocess,threading,time,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
parser=argparse.ArgumentParser();parser.add_argument('--exe',default='native/target/release/vectordash.exe');args=parser.parse_args()
root=pathlib.Path('windows/.smoke/ai-response-layout')/uuid.uuid4().hex;root.mkdir(parents=True)
answer='''The claim in this example cannot be verified from the available source material. This deliberately lengthy introductory paragraph must wrap within the response pane, including when the window is a quick popup rather than the full application.

| Claim | Verdict | Evidence |
|---|---|---|
| An unusually large reported figure | Unverified | No credible source in this synthetic fixture supports the stated figure. More context is needed before drawing a conclusion. |

- **Context:** [A descriptive source title](https://example.org/news/article_(topic)?a=1&b=two#section) should appear as an underlined link, followed by readable text that wraps naturally rather than continuing beyond the right edge of the window.
- **Raw source:** https://example.net/a/very/long/source/path?query=value&other=one should appear as a compact clickable source while preserving the original full address.
- **Code remains literal:** `https://example.org/code`.

Conclusion: These examples test formatting only and make no factual claims.
'''
mode='json';requests=[]
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_POST(self):
  requests.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
  self.send_response(200)
  if mode=='stream':
   self.send_header('Content-Type','text/event-stream');self.end_headers()
   for start in range(0,len(answer),65):
    self.wfile.write(('data: '+json.dumps({'choices':[{'delta':{'content':answer[start:start+65]}}]})+'\n\n').encode());self.wfile.flush();time.sleep(.025)
   self.wfile.write(b'data: [DONE]\n\n')
  else:
   data=json.dumps({'choices':[{'message':{'role':'assistant','content':answer}}]}).encode();self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
try:
 for quick in [False,True]:
  for mode in ['json','stream']:
   name=('popup' if quick else 'main')+'-'+mode;folder=(root/name).resolve();profile=folder/'profile';profile.mkdir(parents=True)
   settings={'encrypt_index':False,'roots':[],'include_programs':False,'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'language':'en','sidebar_collapsed':True,'default_search_mode':'Names','llm_enabled':True,'llm_url':f'http://127.0.0.1:{server.server_port}/v1','llm_model':'fixture','llm_context':8192,'llm_response':'Full','clipboard_enabled':False}
   (profile/'settings.json').write_text(json.dumps(settings),encoding='utf-8')
   command=[str(pathlib.Path(args.exe).resolve()),'--smoke-tool','AI','--smoke-followup','Show the formatting fixture','--smoke-seconds','7','--smoke-keep-open','--smoke-report',str(folder/'report.json'),'--smoke-screenshot',str(folder/'view.png')]
   if quick:command+=['--smoke-quick']
   subprocess.run(command,env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=30,check=True)
   report=json.loads((folder/'report.json').read_text());assert report['ai_answer']==answer and not report['ai_busy'],report
   assert (folder/'view.png').stat().st_size>10000
   print(name,'PASS',flush=True)
 for quick in [False,True]:
  name=('popup' if quick else 'main')+'-settled';folder=(root/name).resolve();profile=folder/'profile';profile.mkdir(parents=True)
  (profile/'settings.json').write_text(json.dumps(settings),encoding='utf-8')
  command=[str(pathlib.Path(args.exe).resolve()),'--smoke-tool','AI','--smoke-ai-answer',answer,'--smoke-seconds','4','--smoke-keep-open','--smoke-report',str(folder/'report.json'),'--smoke-screenshot',str(folder/'view.png')]
  if quick:command+=['--smoke-quick']
  subprocess.run(command,env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=30,check=True)
  report=json.loads((folder/'report.json').read_text());assert report['ai_answer']==answer and report['window_size'][1]>500,report
  from PIL import Image
  assert Image.open(folder/'view.png').height>500,'Screenshot captured before expansion'
  print(name,'PASS',flush=True)
 print('PASS: six GUI response cases; artifacts:',root.resolve(),flush=True)
finally:server.shutdown()
