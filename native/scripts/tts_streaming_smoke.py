"""Isolated GUI regression: sentence speech starts before streamed answer completes."""
import argparse,io,json,os,pathlib,subprocess,threading,time,uuid,wave
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');a=p.parse_args()
root=(pathlib.Path('windows/.smoke/tts-incremental')/uuid.uuid4().hex).resolve();root.mkdir(parents=True)
audio=io.BytesIO()
with wave.open(audio,'wb') as wav:
 wav.setnchannels(1);wav.setsampwidth(2);wav.setframerate(24000);wav.writeframes(b'\0\0'*2400)
state={};first='This is the first complete sentence for streamed speech.';second='This second sentence arrives later in the generated answer.'
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  self.send_response(200);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(json.dumps({'data':[{'id':'fixture'}]}).encode())
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  if self.path.endswith('/chat/completions'):
   state['start']=time.monotonic();self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
   for text in [first+' ',second+' ']:
    self.wfile.write(('data: '+json.dumps({'choices':[{'delta':{'content':text}}]})+'\n\n').encode());self.wfile.flush();time.sleep(3)
   state['end']=time.monotonic();self.wfile.write(b'data: [DONE]\n\n');self.wfile.flush()
  elif self.path.endswith('/audio/speech'):
   state['speech'].append({'at':time.monotonic(),'body':body})
   if body.get('stream_format'):
    self.send_response(400);self.end_headers();self.wfile.write(b'unsupported stream format');return
   data=audio.getvalue();self.send_response(200);self.send_header('Content-Type','audio/wav');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
  else:self.send_response(404);self.end_headers()
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
try:
 for enabled in [True,False]:
  state.clear();state['speech']=[];profile=root/str(enabled);profile.mkdir()
  config={'encrypt_index':False,'roots':[],'include_programs':False,'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'language':'en','clipboard_enabled':False,'llm_enabled':True,'llm_url':f'http://127.0.0.1:{server.server_port}/v1','llm_model':'fixture','llm_context':8192,'tts_enabled':True,'tts_auto':True,'tts_streaming':enabled,'tts_url':f'http://127.0.0.1:{server.server_port}/v1','tts_model':'fixture','tts_voice':'fixture'}
  (profile/'settings.json').write_text(json.dumps(config),encoding='utf-8')
  report=profile/'report.json'
  subprocess.run([str(pathlib.Path(a.exe).resolve()),'--smoke-tool','AI','--smoke-followup','Give two sentences.','--smoke-seconds','14','--smoke-keep-open','--smoke-report',str(report)],env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=35,check=True)
  data=json.loads(report.read_text());assert not data.get('ai_error'),data.get('ai_error');assert data['ai_answer'].strip()==first+' '+second
  accepted=[x for x in state['speech']if not x['body'].get('stream_format')];assert accepted,state
  if enabled:
   assert accepted[0]['at']<state['end']-1,state
   assert [x['body']['input'].strip()for x in accepted]==[first,second],accepted
  else:
   assert accepted[0]['at']>=state['end'],state
   assert len(accepted)==1 and not any(x['body'].get('stream_format')for x in state['speech']),state
  (profile/'timing.json').write_text(json.dumps(state,indent=2))
  print('PASS streaming',enabled,'first speech after',round(accepted[0]['at']-state['start'],2),'s; response finished after',round(state['end']-state['start'],2),'s',flush=True)
 print('Artifacts:',root,flush=True)
finally:server.shutdown()
