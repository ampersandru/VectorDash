"""Disposable browser readiness and embedded-widget discovery/navigation fixture."""
import argparse,json,os,pathlib,queue,subprocess,threading,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');a=p.parse_args()
root=pathlib.Path('windows/.smoke/browser-research').resolve()/uuid.uuid4().hex;root.mkdir(parents=True)
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  self.send_response(200);self.send_header('Content-Type','text/html');self.end_headers()
  if self.path=='/shell':html='<title>Embedded booking fixture</title><iframe title="Booking widget" src="/widget"></iframe>'
  elif self.path=='/widget':html='<title>Booking terms fixture</title><p>Fixture only. Cancellation is free until 24 hours before the reservation. No credit card required.</p><button>Review only</button>'
  elif self.path=='/hydrate':html='<title>Loading fixture</title><body><script>setTimeout(()=>{document.body.innerHTML="<p>Fixture menu: pasta $25. Cancellation terms unverified.</p><button>View availability</button>"},300)</script>'
  else:html='<title>Empty fixture</title><body></body>'
  self.wfile.write(html.encode())
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
worker=subprocess.Popen([str(pathlib.Path(a.exe).resolve()),'--control-worker'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,encoding='utf-8')
responses=queue.Queue()
def reader():
 for line in worker.stdout:responses.put(line)
threading.Thread(target=reader,daemon=True).start()
def call(action,**args):
 worker.stdin.write(json.dumps({'tool':'browser_use','args':{'action':action,**args},'vision':True,'owner_pid':os.getpid(),'browser_path':'C:/Program Files/Google/Chrome/Application/chrome.exe','browser_visible':False,'browser_profile':str(root/'profile')})+'\n');worker.stdin.flush()
 result=json.loads(responses.get(timeout=25));assert 'error' not in result,result;return result
try:
 base=f'http://127.0.0.1:{server.server_port}'
 snap=call('open',url=base+'/shell')
 assert snap['page_state']=='embedded_content',snap
 assert len(snap['embedded_frames'])==1 and snap['embedded_frames'][0]['url']==base+'/widget',snap
 assert 'No credit card' not in snap['text'],'Frame contents must not be claimed as inspected'
 snap=call('navigate',url=snap['embedded_frames'][0]['url'])
 assert snap['page_state']=='ready' and 'No credit card required' in snap['text'],snap
 snap=call('navigate',url=base+'/hydrate')
 assert snap['page_state']=='ready' and 'pasta $25' in snap['text'],snap
 snap=call('navigate',url=base+'/empty')
 assert snap['page_state']=='empty' and 'observation_note' in snap,snap
 call('close');print('PASS embedded-widget discovery, explicit navigation, bounded hydration wait and empty-page reporting',flush=True)
 print('Artifacts:',root,flush=True)
finally:
 worker.terminate();worker.wait();server.shutdown()
