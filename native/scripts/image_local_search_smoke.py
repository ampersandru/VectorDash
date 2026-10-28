"""Isolated reverse-image and AI Local Search GUI integration checks."""
import argparse,base64,io,json,os,pathlib,sqlite3,struct,subprocess,threading,time
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from PIL import Image
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/release/vectordash.exe');p.add_argument('--case');a=p.parse_args()
root=pathlib.Path('windows/.smoke/image-local-search').resolve();root.mkdir(parents=True,exist_ok=True)
fixture=root/'query.png';Image.new('RGB',(180,100),(210,60,35)).save(fixture)
calls=[]
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):self.reply({'data':[{'id':'fixture'}]})
 def reply(self,body):
  data=json.dumps(body).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));calls.append((self.path,body))
  if self.path.endswith('/embeddings'):
   assert body['input'][0]['content'][0]['type']=='image_url',body
   self.reply({'data':[{'embedding':[1.,0.]}]});return
  if any(m['role']=='tool' for m in body['messages']):
   assert 'actual.png' in body['messages'][-1]['content'],body
   self.reply({'choices':[{'message':{'content':'Found actual.png in the local index.'}}]});return
  assert {t['function']['name'] for t in body['tools']}=={'local_search','web_search'},body
  self.reply({'choices':[{'message':{'content':None,'tool_calls':[{'id':'local-1','type':'function','function':{'name':'local_search','arguments':json.dumps({'query':'name:actual.png'})}}]}}]})
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);base='http://127.0.0.1:'+str(server.server_port);threading.Thread(target=server.serve_forever,daemon=True).start()
exe=str(pathlib.Path(a.exe).resolve())
def run(folder,flags=()):
 report=folder/'report.json'
 subprocess.run([exe,'--smoke-report',str(report),'--smoke-screenshot',str(folder/'view.png'),'--smoke-seconds','4','--smoke-keep-open',*flags],env={**os.environ,'VECTORDASH_DATA_HOME':str(folder/'profile')},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=25,check=True)
 return json.loads(report.read_text())
try:
 for case,tool,submit in [('search-ready','Search',False),('search-results','Search',True),('clipboard-results','Clipboard',True),('ai-local','AI',False),('disabled','Search',True),('search-auto','Search',True),('clipboard-auto','Clipboard',True),('search-early-enter','Search',True),('clipboard-early-enter','Clipboard',True),('search-drop','Search',True),('clipboard-drop','Clipboard',True)]:
  if a.case and a.case!=case:continue
  calls.clear();folder=root/case;profile=folder/'profile';profile.mkdir(parents=True,exist_ok=True)
  sources=folder/'photos';sources.mkdir(exist_ok=True)
  settings={'language':'en','encrypt_index':False,'watch_changes':False,'auto_check_updates':False,'include_programs':False,'mcp_enabled':False,'default_search_mode':'Names','roots':[{'path':str(sources),'mode':'quick' if tool=='AI' else 'full'}],'embedding_server':'Remote','remote_url':base+'/v1','remote_model':'embeddinggemma2','remote_warm_on_open':False,'image_search_enabled':case!='disabled','image_search_auto':case.endswith('-auto') or case=='disabled','clipboard_enabled':True,'clipboard_image_embeddings':True,'clipboard_hours':0,'clipboard_options':{'capture_text':False,'capture_images':False,'max_items':0,'max_mb':0},'llm_enabled':True,'llm_url':base+'/v1','llm_model':'fixture','llm_context':8192}
  if tool=='AI':settings.update(ai_web_default=True,web={'enabled':True,'searxng_url':base})
  (profile/'settings.json').write_text(json.dumps(settings),encoding='utf-8')
  run(folder)
  with sqlite3.connect(profile/'index.sqlite') as db:
   for table in ['names','segments','segment_vectors']:db.execute('DELETE FROM '+table)
   for i,(name,x) in enumerate([('actual.png',1.)]+[(f'other{i:02}.png',0.1) for i in range(19)]):
    path=sources/name;Image.new('RGB',(180,100),(210,60,35) if i==0 else (20,60,210)).save(path)
    db.execute('INSERT OR REPLACE INTO names(path,name,parent,kind,folded,base,mtime,size,index_mode) VALUES(?,?,?,?,?,?,?,?,?)',(str(path),name,str(sources),'file',str(path).lower(),name.lower(),1,100,'full' if tool!='AI' else 'quick'))
    db.execute("INSERT INTO segments VALUES(?,'files','image','','[]',?)",(str(path),json.dumps({'embedding_source':'media'})));rid=db.execute('SELECT last_insert_rowid()').fetchone()[0]
    db.execute("INSERT INTO segment_vectors VALUES(?,'files','image','image',2,?,?)",(rid,struct.pack('<ff',x,(1-x*x)**.5),str(path)))
   db.execute("INSERT INTO name_words(name_words) VALUES('rebuild')")
  with sqlite3.connect(profile/'clipboard.sqlite') as db:
   db.execute("CREATE TABLE IF NOT EXISTS clips(id INTEGER PRIMARY KEY,created INTEGER NOT NULL,payload TEXT NOT NULL,digest TEXT NOT NULL UNIQUE,kind TEXT DEFAULT 'text',bytes INTEGER DEFAULT 0,embedding TEXT DEFAULT '',model_key TEXT DEFAULT '',pinned INTEGER DEFAULT 0,sticky INTEGER DEFAULT 0,title TEXT DEFAULT '',tags TEXT DEFAULT '',ocr TEXT DEFAULT '',ocr_done INTEGER DEFAULT 0)")
   db.execute('DELETE FROM clips')
   cols={r[1] for r in db.execute('pragma table_info(clips)')};assert 'embedding' in cols
   key='||'+base+'/v1|embeddinggemma2|'
   payload=base64.b64encode(fixture.read_bytes()).decode()
   for i in range(605):db.execute("INSERT INTO clips(created,payload,digest,kind,embedding,model_key,pinned,sticky,title) VALUES(?,?,?,'image',?,?,?,0,'')",(int(time.time())+i,payload,str(i),json.dumps([1,0] if i==0 else [0,1]),key,int(i==0)))
  flags=['--smoke-tool',tool,'--smoke-quick']
  if tool=='AI':flags+=['--smoke-local-ai','--smoke-query','Find my image']
  else:flags+=[('--smoke-drop-image' if case.endswith('-drop') else '--smoke-image-query'),str(fixture)]+([] if case.endswith('-auto') else ['--smoke-image-early-enter'] if case.endswith('-early-enter') else ['--smoke-image-submit'] if submit else [])
  r=run(folder,flags)
  if case=='disabled':assert not r['image_query_ready'] and not calls,r
  elif tool=='AI':assert r['ai_web_enabled'] and r['ai_local_enabled'] and r['ai_answer']=='Found actual.png in the local index.',r
  elif not submit:assert r['image_query_ready'] and not r['image_search_submitted'] and not calls,r
  elif tool=='Search':assert r['image_results'][0][0].endswith('actual.png') and len(calls)==1 and r['selected']==0 and r['result_scroll_offset']<1,r
  else:assert r['clipboard_image_results'][0]==[1,True] and len(calls)==1,r
  print('PASS',case,flush=True)
finally:server.shutdown()
