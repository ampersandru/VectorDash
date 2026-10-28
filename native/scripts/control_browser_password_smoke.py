"""Controlled localhost login: password entry, Enter submission and masked observations."""
import argparse,json,pathlib,queue,subprocess,threading,time,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');p.add_argument('--browser-path',default='C:/Program Files/Google/Chrome/Application/chrome.exe');a=p.parse_args()
root=(pathlib.Path('windows/.smoke/browser-password')/uuid.uuid4().hex).resolve();root.mkdir(parents=True)
received=[]
html=b"""<!doctype html><meta charset=utf-8><title>Local password fixture</title><h1>Disposable login</h1><form id=form><input aria-label=Message id=message><input aria-label=Password id=password type=password autocomplete=current-password><input aria-label='Alternate password' id=alternate autocomplete=new-password><button>Sign in</button></form><p id=result>Waiting</p><p id=echo></p><script>password.addEventListener('input',()=>{echo.textContent=password.value});form.addEventListener('submit',async e=>{e.preventDefault();await fetch('/verify',{method:'POST',body:JSON.stringify({password:password.value})});result.textContent='Signed in'});</script>"""
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):self.send_response(200);self.send_header('Content-Type','text/html');self.send_header('Content-Length',str(len(html)));self.end_headers();self.wfile.write(html)
 def do_POST(self):
  received.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))));self.send_response(200);self.end_headers();self.wfile.write(b'ok')
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
worker=subprocess.Popen([str(pathlib.Path(a.exe).resolve()),'--control-worker'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,encoding='utf-8');responses=queue.Queue()
def reader():
 for line in worker.stdout:responses.put(line)
threading.Thread(target=reader,daemon=True).start()
def call(action,reject=False,**args):
 worker.stdin.write(json.dumps({'tool':'browser_use','args':{'action':action,**args},'vision':False,'owner_pid':0,'browser_path':a.browser_path,'browser_visible':False,'browser_profile':str(root/'profile')})+'\n');worker.stdin.flush();result=json.loads(responses.get(timeout=30))
 assert ('error'in result)if reject else('error'not in result),result
 return result
try:
 snap=call('open',url=f'http://127.0.0.1:{server.server_port}/')
 field=next(e for e in snap['elements']if e['name']=='Message')
 call('type',reject=True,snapshot_id=snap['snapshot_id'],element=field['id'],password='must-not-fill-normal-field')
 snap=call('snapshot');assert next(e for e in snap['elements']if e['name']=='Message')['value']==''
 secret='Fixture-'+uuid.uuid4().hex+'-日本語-<quote>"'
 field=next(e for e in snap['elements']if e['name']=='Password');old=snap['snapshot_id']
 result=call('type',snapshot_id=old,element=field['id'],password=secret);assert result['performed'] and result['result']['sensitive']
 call('type',reject=True,snapshot_id=old,element=field['id'],password='stale-must-not-land')
 snap=call('snapshot');field=next(e for e in snap['elements']if e['name']=='Password');assert field['value']=='' and field['password'] and field['has_value'];assert secret not in json.dumps(snap,ensure_ascii=False)
 call('key',snapshot_id=snap['snapshot_id'],keys='Enter')
 deadline=time.monotonic()+5
 while not received and time.monotonic()<deadline:time.sleep(.05)
 assert received and received[-1]['password']==secret,'Local login did not receive the exact credential'
 snap=call('snapshot')
 while 'Signed in'not in snap['text'] and time.monotonic()<deadline:
  assert secret not in json.dumps(snap,ensure_ascii=False);time.sleep(.05);snap=call('snapshot')
 assert 'Signed in'in snap['text'] and secret not in json.dumps(snap,ensure_ascii=False)
 alternate=next(e for e in snap['elements']if e['name']=='Alternate password');assert alternate['password']
 second='Alternate-'+uuid.uuid4().hex
 call('type',snapshot_id=snap['snapshot_id'],element=alternate['id'],text=second)
 snap=call('snapshot');alternate=next(e for e in snap['elements']if e['name']=='Alternate password');assert alternate['value']=='' and alternate['has_value'];assert second not in json.dumps(snap)
 call('navigate',reject=True,url='javascript:alert(1)');call('close')
 print('PASS localhost password submission, autocomplete fields, stale/normal-target rejection, masked values/echoes, Enter, and restricted URL checks',flush=True)
finally:
 worker.stdin.close()
 try:worker.wait(timeout=4)
 except subprocess.TimeoutExpired:worker.kill();worker.wait()
 server.shutdown()
