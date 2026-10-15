"""Disposable 24-image preparation/inference overlap benchmark; requires Pillow."""
import json,subprocess,os,tempfile,threading,time,statistics,argparse,sqlite3
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from PIL import Image
parser=argparse.ArgumentParser();parser.add_argument('--exe',required=True);parser.add_argument('--baseline',required=True);parser.add_argument('--output',required=True);args=parser.parse_args()
root=Path(tempfile.mkdtemp(prefix='VD-prefetch-'));source=root/'photos';source.mkdir()
for i in range(24):Image.new('RGB',(1600,1000),(i*10,80,120)).save(source/f'photo-{i}.png')
calls=0
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_POST(self):
  global calls
  calls+=1
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));items=body['input'];items=items if isinstance(items,list)else[items];time.sleep(.08);b=json.dumps({'data':[{'index':i,'embedding':[1,0,0]}for i in range(len(items))]}).encode();self.send_response(200);self.send_header('Content-Length',str(len(b)));self.end_headers();self.wfile.write(b)
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();report=[]
baseline=Path(args.baseline).resolve()
try:
 for label,exe in [('before',baseline),('after',Path(args.exe).resolve())]:
  runs=[];requests=[]
  for i in range(3):
   calls=0
   home=root/f'{label}-{i}';home.mkdir();s={'encrypt_index':False,'embedding_server':'Remote','remote_url':f'http://127.0.0.1:{server.server_port}/v1','remote_model':'fixture','include_programs':False,'watch_changes':False,'check_cpu':False,'check_gpu':False,'check_user_idle':False,'pause_for_game':False,'pause_for_fullscreen':False,'ac_only':False,'defer_search_model':False,'batch_size':4,'ocr_enabled':False};(home/'settings.json').write_text(json.dumps(s));env=os.environ.copy();env['VECTORDASH_DATA_HOME']=str(home);p=subprocess.run([str(exe),'--index',str(source),'--unencrypted'],env=env,capture_output=True,text=True,timeout=90);assert p.returncode==0,p.stderr;d=json.loads(p.stdout);runs.append(d['index_seconds']);requests.append(calls)
   c=sqlite3.connect(home/'index.sqlite');assert c.execute('SELECT count(*) FROM segment_vectors').fetchone()[0]==24;assert c.execute("SELECT count(*) FROM native_content WHERE error<>''").fetchone()[0]==0;c.close()
  row={'build':label,'median_s':statistics.median(runs),'runs':runs,'requests':requests,'indexed_images':24};report.append(row);print(row,flush=True)
finally:server.shutdown();Path(args.output).write_text(json.dumps(report,indent=2))
