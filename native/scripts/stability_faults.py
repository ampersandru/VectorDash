"""Disposable end-to-end interruption, outage, and Git-hook regression checks."""
import argparse,json,os,sqlite3,subprocess,tempfile,threading,time
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
parser=argparse.ArgumentParser();parser.add_argument('--exe',required=True);parser.add_argument('--output',required=True);args=parser.parse_args()
exe=str(Path(args.exe).resolve());root=Path(tempfile.mkdtemp(prefix='VectorDash-faults-'));state={'fail':False,'delay':0,'calls':0}
class Handler(BaseHTTPRequestHandler):
 def do_POST(self):
  data=json.loads(self.rfile.read(int(self.headers['Content-Length'])));items=data['input'];items=items if isinstance(items,list) else [items];state['calls']+=1;time.sleep(state['delay'])
  bad=any('REJECTFIXTURE' in str(item) for item in items)
  body=json.dumps({'error':'temporary outage'} if state['fail'] else {'error':'invalid input fixture'} if bad else {'data':[{'index':i,'embedding':[1,0,0]} for i in range(len(items))]}).encode();self.send_response(503 if state['fail'] else 400 if bad else 200);self.send_header('Content-Length',str(len(body)));self.end_headers()
  try:self.wfile.write(body)
  except OSError:pass
 def log_message(self,*a):pass
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
def setup(name):
 h=root/name;h.mkdir();source=root/(name+'-source');source.mkdir();s={'encrypt_index':False,'embedding_server':'Remote','remote_url':f'http://127.0.0.1:{server.server_port}/v1','remote_model':'fixture','include_programs':False,'watch_changes':False,'check_cpu':False,'check_gpu':False,'check_user_idle':False,'pause_for_game':False,'pause_for_fullscreen':False,'ac_only':False,'defer_search_model':False,'batch_size':4};(h/'settings.json').write_text(json.dumps(s));return h,source
def start(h,source,quick=False):
 env=os.environ.copy();env['VECTORDASH_DATA_HOME']=str(h);return subprocess.Popen([exe,'--index',str(source),'--unencrypted']+(['--quick'] if quick else []),env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8')
def finish(p):
 out,err=p.communicate(timeout=90);assert p.returncode==0,err;return json.loads(out)
def count(h,sql):
 with sqlite3.connect(h/'index.sqlite',timeout=.2) as c:return c.execute(sql).fetchone()[0]
report={}
try:
 h,source=setup('resume');(source/'large.txt').write_text(('A garden has seasonal vegetables and flowers. '*40+'\n')*120,encoding='utf-8');state['delay']=.06;p=start(h,source);deadline=time.monotonic()+40;staged=0
 while time.monotonic()<deadline and p.poll() is None:
  try:staged=count(h,'SELECT count(*) FROM native_staging')
  except sqlite3.Error:pass
  if staged>=8:break
  time.sleep(.02)
 
 if staged<8:
  out,err=p.communicate();raise AssertionError('No durable checkpoint: '+out[-3000:]+err[-500:])
 p.kill();p.communicate();assert count(h,'SELECT count(*) FROM segments')==0,'Partial content published'
 state['calls']=0;state['delay']=0;finish(start(h,source));remaining=state['calls'];vectors=count(h,'SELECT count(*) FROM segment_vectors');assert vectors>staged;assert count(h,'SELECT count(*) FROM native_jobs')==0
 fresh,fresh_source=setup('uninterrupted');(fresh_source/'large.txt').write_bytes((source/'large.txt').read_bytes());state['calls']=0;finish(start(fresh,fresh_source));full_requests=state['calls'];assert remaining<full_requests,(remaining,full_requests)
 report['interrupted_resume']={'staged_before_kill':staged,'final_vectors':vectors,'requests_after_restart':remaining,'uninterrupted_requests':full_requests,'partial_content_visible':False}
 h,source=setup('outage');(source/'document.txt').write_text('A document about seasonal gardening.');state['fail']=True;failed=start(h,source);out,err=failed.communicate(timeout=90);assert failed.returncode!=0 and '503' in err;assert count(h,'SELECT count(*) FROM native_jobs')==1;assert count(h,'SELECT count(*) FROM segment_vectors')==0
 state['fail']=False;finish(start(h,source));assert count(h,"SELECT count(*) FROM native_content WHERE error='' ")==1;assert count(h,'SELECT count(*) FROM native_jobs')==0;report['transient_outage_recovered']=True
 h,source=setup('bad-input')
 for i in range(4):(source/f'good-{i}.txt').write_text('A readable garden document.')
 (source/'bad.txt').write_text('REJECTFIXTURE');finish(start(h,source));assert count(h,"SELECT count(*) FROM native_content WHERE error=''")==4;assert count(h,"SELECT count(*) FROM native_content WHERE error!=''")==1;report['bad_input_isolated']=True
 h,source=setup('git-hook');subprocess.run(['git','init','-q',str(source)],check=True);marker=source/'hook-ran';hook=source/'hook.cmd';hook.write_text('@echo hook>'+str(marker));subprocess.run(['git','-C',str(source),'config','core.fsmonitor',str(hook)],check=True);(source/'ordinary.txt').write_text('ordinary');finish(start(h,source,True));assert not marker.exists(),'Indexer executed Git hook';report['git_fsmonitor_not_executed']=True
 print(json.dumps(report,indent=2))
finally:
 Path(args.output).write_text(json.dumps(report,indent=2));server.shutdown()
