"""Measure real release searches and FP16/FP32 ranking agreement in a disposable index."""
import argparse,json,math,os,random,sqlite3,statistics,struct,subprocess,tempfile,threading,time
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import psutil
p=argparse.ArgumentParser();p.add_argument('--exe',required=True);p.add_argument('--output',required=True);p.add_argument('--files',type=int,default=20000);args=p.parse_args();exe=Path(args.exe).resolve();root=Path(tempfile.mkdtemp(prefix='VectorDash-cache-bench-'));home=root/'home';home.mkdir();source=root/'catalog';source.mkdir();rng=random.Random(941)
def normalized(values):
 scale=math.sqrt(sum(x*x for x in values));return [x/scale for x in values]
probes=[normalized([rng.gauss(0,1)for _ in range(768)])for _ in range(8)]
class Handler(BaseHTTPRequestHandler):
 protocol_version='HTTP/1.1'
 def do_POST(self):
  import socket
  self.connection.setsockopt(socket.IPPROTO_TCP,socket.TCP_NODELAY,1)
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));text=str(body['input']);index=next((i for i in range(8)if f'probe{i}'in text),0);data=json.dumps({'data':[{'index':0,'embedding':probes[index]}]}).encode();self.send_response(200);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
 def log_message(self,*a):pass
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
settings={'encrypt_index':False,'roots':[{'path':str(source),'mode':'full'}],'embedding_server':'Remote','remote_url':f'http://127.0.0.1:{server.server_port}/v1','remote_model':'fixture','include_programs':False,'auto_check_updates':False,'watch_changes':False,'pause_for_game':False,'pause_for_fullscreen':False,'defer_search_model':False,'rank_calibration':False}
def save():(home/'settings.json').write_text(json.dumps(settings),encoding='utf-8')
def run(*argv):
 env=os.environ.copy();env['VECTORDASH_DATA_HOME']=str(home);process=subprocess.Popen([str(exe),*argv],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8');peak=0;stop=threading.Event()
 def sample():
  nonlocal peak
  while not stop.wait(.01):
   try:peak=max(peak,psutil.Process(process.pid).memory_info().rss)
   except psutil.Error:pass
 thread=threading.Thread(target=sample);thread.start()
 try:out,err=process.communicate(timeout=120)
 finally:stop.set();thread.join()
 assert process.returncode==0,err;return json.loads(out),round(peak/1048576,2)
save();run('--query','','--mode','Names');db=sqlite3.connect(home/'index.sqlite');db.execute('BEGIN');location=json.dumps({'page':0,'seconds':-1,'line':0,'embedding_source':'text'})
for i in range(args.files):
 # Most rows are unrelated; nearby candidates stress close-ranking precision without artificial ties.
 if i<8:vector=probes[i]
 elif i<808:vector=normalized([v+rng.gauss(0,.003)for v in probes[i%8]])
 else:vector=normalized([rng.gauss(0,1)for _ in range(768)])
 path=str(source/f'file-{i:06d}.txt');name=Path(path).name;blob=struct.pack('<768f',*vector)
 db.execute('INSERT INTO names VALUES(?,?,?,?,?,?,?,?,?)',(path,name,str(source),'file',path.lower(),name.lower(),1,2048,'full'));db.execute('INSERT INTO segments VALUES(?,?,?,?,?,?)',(path,'files','text','unrelated fixture document','[]',location));rid=db.execute('SELECT last_insert_rowid()').fetchone()[0];db.execute('INSERT INTO segment_vectors VALUES(?,?,?,?,?,?,?)',(rid,'files','text','text',768,blob,path))
db.execute("INSERT INTO name_words(name_words) VALUES('rebuild')");db.commit();db.close();report={'files':args.files,'dimensions':768,'rows':[],'comparisons':[],'limits':['Synthetic normalized vectors; pipeline and cache comparison, not an inference benchmark.','OS disk cache not flushed. RAM is backend process RSS, excludes server and VRAM.']};results={}
try:
 for encrypted in [False,True]:
  if encrypted:run('--convert-encryption','on');settings['encrypt_index']=True
  for profile in ['Low memory','Balanced','Fast search']:
   for precision in ['FP32','FP16']:
    settings.update(performance=profile,cache_precision=precision);save()
    for mode in ['Names','Semantic','Both']:
     times=[];peak=0;checks=[]
     for index in range(4 if mode!='Names' else 1):
      query='file-000000' if mode=='Names' else f'probe{index}'
      data,rss=run('--query',query,'--mode',mode,'--repeat','8');times+=data['search_times_ms'][1:];peak=max(peak,rss);checks.append(data['results']);results[(encrypted,profile,precision,mode,index)]=data['results']
     row={'encrypted':encrypted,'profile':profile,'precision':precision,'mode':mode,'warm_median_ms':round(statistics.median(times),3),'warm_p95_ms':round(sorted(times)[math.ceil(len(times)*.95)-1],3),'peak_rss_mib':peak};report['rows'].append(row);print(json.dumps(row),flush=True)
 for encrypted in [False,True]:
  for profile in ['Low memory','Balanced','Fast search']:
   for mode in ['Semantic','Both']:
    overlaps=[];same=0;errors=[]
    for i in range(4):
     a=results[(encrypted,profile,'FP32',mode,i)];b=results[(encrypted,profile,'FP16',mode,i)];same+=a[0]['path']==b[0]['path'];overlaps.append(len({x['path']for x in a[:10]}&{x['path']for x in b[:10]})/10);scores={x['path']:x['score']for x in a};errors.extend(abs(x['score']-scores[x['path']])for x in b if x['path']in scores)
    report['comparisons'].append({'encrypted':encrypted,'profile':profile,'mode':mode,'top1_agreement':same/4,'top10_overlap':statistics.mean(overlaps),'max_shared_score_error':max(errors)})
    assert same==4,'Top result changed on synthetic relevance probes'
finally:
 Path(args.output).write_text(json.dumps(report,indent=2),encoding='utf-8');server.shutdown()
