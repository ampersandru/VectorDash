"""Real Chromium scroll targeting; only a disposable localhost page/profile is used."""
import argparse,json,os,pathlib,queue,subprocess,threading,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');a=p.parse_args()
root=pathlib.Path('windows/.smoke/browser-scroll').resolve()/uuid.uuid4().hex;root.mkdir(parents=True)
html=b"""<meta charset="utf-8"><style>html,body{margin:0;height:100%;overflow:hidden}main{width:70vw;height:60vh;overflow-y:auto}aside{width:10vw;height:20vh;overflow-y:auto}</style><main><button>Content target</button><div style="height:4000px">Main content</div></main><aside><button>Sidebar target</button><div style="height:2000px">Sidebar content</div></aside>"""
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):self.send_response(200);self.send_header('Content-Type','text/html');self.end_headers();self.wfile.write(html)
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
 snap=call('open',url=f'http://127.0.0.1:{server.server_port}/')
 r=call('scroll',snapshot_id=snap['snapshot_id'],delta=240)['scroll_result'];assert r['method']=='dom' and r['moved'] and r['after_position']>0,r
 snap=call('snapshot');target=next(v['id'] for v in snap['elements'] if v['name']=='Content target')
 r=call('scroll',snapshot_id=snap['snapshot_id'],element=target,delta=-240)['scroll_result'];assert r['moved'] and r['after_position']==0 and r['at_boundary'],r
 snap=call('snapshot');target=next(v['id'] for v in snap['elements'] if v['name']=='Content target')
 r=call('scroll',snapshot_id=snap['snapshot_id'],element=target,delta=-240);assert not r['scroll_result']['moved'] and r['scroll_result']['at_boundary'] and 'progress_warning' in r,r
 call('close');print('PASS nested main-pane scrolling, explicit ancestor targeting, direction, measured movement and boundary',flush=True)
finally:
 worker.terminate();worker.wait();server.shutdown()
