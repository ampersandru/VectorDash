"""Disposable settings UI/search navigation fixtures; no user settings or models."""
import argparse,json,os,pathlib,subprocess,threading,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');a=p.parse_args()
root=(pathlib.Path('windows/.smoke/settings-search')/uuid.uuid4().hex).resolve();root.mkdir(parents=True)
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  data=json.dumps({'data':[{'id':'fixture-voice'},{'id':'fixture-dictation'}]}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
cases=[('local-priority',2,'speech',None),('voice-navigation',2,'Enable TTS streaming','Enable TTS streaming'),('clipboard-navigation',4,'History storage','History storage MiB (0 = unlimited)'),('idle-navigation',5,'GPU usage below','GPU usage below (%)'),('remote-status',2,'',None),('remove-speech',2,'',None)]
try:
 for name,page,query,select in cases:
  folder=root/name;profile=folder/'profile';profile.mkdir(parents=True)
  model=profile/'runtimes/local-tts/paradee/paradee-web-v2/models/v2f.pdn';model.parent.mkdir(parents=True);model.write_bytes(b'fixture weights')
  config={'encrypt_index':False,'roots':[],'include_programs':False,'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'language':'en','clipboard_enabled':False,'tts_enabled':True,'tts_local':name!='remote-status','tts_model':'fixture-voice','tts_url':f'http://127.0.0.1:{server.server_port}/v1','dictation_server':'Remote','dictation_model':'fixture-dictation','dictation_url':f'http://127.0.0.1:{server.server_port}/v1'}
  (profile/'settings.json').write_text(json.dumps(config),encoding='utf-8')
  args=[str(pathlib.Path(a.exe).resolve()),'--smoke-page',str(page),'--smoke-settings-query',query,'--smoke-seconds','6','--smoke-screenshot',str(folder/'view.png'),'--smoke-report',str(folder/'report.json')]
  if select:args+=['--smoke-settings-select',select]
  if name=='remove-speech':args+=['--smoke-remove-component','local-tts']
  subprocess.run(args,env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=30,check=True)
  report=json.loads((folder/'report.json').read_text());search=report['settings_search']
  if name=='local-priority':
   rows=search['results'];assert rows and rows[0]['page']==2;split=next(i for i,r in enumerate(rows)if r['page']!=2);assert all(r['page']!=2 for r in rows[split:]);assert any(r['page']==5 for r in rows)
  if select:
   expected={'voice-navigation':(5,'Voice Model'),'clipboard-navigation':(9,'Retention and storage'),'idle-navigation':(3,'Idle conditions')}[name]
   assert (search['page'],search['section'])==expected and not search['pending'],search
   assert not search['modified'],'Navigation must not change settings'
  assert model.is_file(),'UI fixture must not uninstall files without confirmation'
  print('PASS',name,flush=True)
 print('Artifacts:',root,flush=True)
finally:server.shutdown()
