"""Isolated GUI regression: sentence speech starts before streamed answer completes."""
import argparse,io,json,os,pathlib,subprocess,threading,time,uuid,wave
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');a=p.parse_args()
root=(pathlib.Path('windows/.smoke/tts-dismissal')/uuid.uuid4().hex).resolve();root.mkdir(parents=True)
audio=io.BytesIO()
with wave.open(audio,'wb') as wav:
 wav.setnchannels(1);wav.setsampwidth(2);wav.setframerate(24000);wav.writeframes(b'\0\0'*72000)
short_audio=io.BytesIO()
with wave.open(short_audio,'wb') as wav:
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
   for text in ['This is the [first][source] complete **sentence** for streamed speech. ',second+' ']:
    self.wfile.write(('data: '+json.dumps({'choices':[{'delta':{'content':text}}]})+'\n\n').encode());self.wfile.flush();time.sleep(3)
   self.wfile.write(('data: '+json.dumps({'choices':[{'delta':{'content':'\n\n[source]: https://example.org/hidden?tracking=1\n'}}]})+'\n\n').encode());self.wfile.flush()
   state['end']=time.monotonic();self.wfile.write(b'data: [DONE]\n\n');self.wfile.flush()
  elif self.path.endswith('/audio/speech'):
   state['speech'].append({'at':time.monotonic(),'body':body})
   if body.get('stream_format'):
    self.send_response(400);self.end_headers();self.wfile.write(b'unsupported stream format');return
   data=short_audio.getvalue() if mode=="escape" else audio.getvalue();self.send_response(200);self.send_header('Content-Type','audio/wav');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
  else:self.send_response(404);self.end_headers()
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
try:
 for mode in ["outside","escape","stop","protected"]:
  enabled=False
  state.clear();state['speech']=[];profile=root/mode;profile.mkdir()
  config={'encrypt_index':False,'roots':[],'include_programs':False,'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'language':'en','clipboard_enabled':False,'llm_enabled':True,'llm_url':f'http://127.0.0.1:{server.server_port}/v1','llm_model':'fixture','llm_context':8192,'tts_enabled':True,'tts_auto':True,'tts_streaming':enabled,'tts_url':f'http://127.0.0.1:{server.server_port}/v1','tts_model':'fixture','tts_voice':'fixture'}
  (profile/'settings.json').write_text(json.dumps(config),encoding='utf-8')
  report=profile/'report.json'
  subprocess.run([str(pathlib.Path(a.exe).resolve()),'--smoke-quick','--smoke-tts-'+('escape' if mode=='escape' else 'protected' if mode=='protected' else 'outside'),*(['--smoke-tts-stop']if mode=='stop'else []),'--smoke-tool','AI','--smoke-followup','Give two sentences.','--smoke-seconds','22','--smoke-keep-open','--smoke-report',str(report)],env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=35,check=True)
  data=json.loads(report.read_text());assert data.get('closed') and data.get('quick') and not data.get('tts_busy'),data
  assert state['speech'],'Speech request must have been dispatched'
  if mode=='protected':assert data.get('ai_held') and data.get('speech_held'),data
  print('PASS popup dismissal after speech:',mode,flush=True)
 print('Artifacts:',root,flush=True)
finally:server.shutdown()
