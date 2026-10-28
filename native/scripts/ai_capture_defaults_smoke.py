"""Isolated Windows GUI regressions; never reads the installed user profile."""
import argparse, io, json, os, pathlib, subprocess, threading, urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from PIL import Image

parser=argparse.ArgumentParser()
parser.add_argument('--exe',default='native/target/release/vectordash.exe')
args=parser.parse_args()
root=pathlib.Path('windows/.smoke/ai-capture-defaults').resolve()
root.mkdir(parents=True,exist_ok=True)
image=io.BytesIO();Image.new('RGB',(640,360),(40,140,190)).save(image,format='PNG')
fixture=root/'capture.png';fixture.write_bytes(image.getvalue())
requests=[]
class Handler(BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def reply(self,value):
        data=json.dumps(value).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
    def do_GET(self):
        path=urllib.parse.urlsplit(self.path).path
        if path=='/thumb.png':
            self.send_response(200);self.send_header('Content-Type','image/png');self.end_headers();self.wfile.write(image.getvalue());return
        if path.endswith('/models'):self.reply({'data':[{'id':'fixture'}]});return
        if path=='/search':
            self.reply({'results':[{'title':'Long result title '*12,'url':'https://example.org/'+('long-path/'*20),'content':'A lengthy search result description must stay beside its thumbnail. '*12,'thumbnail':base+'/thumb.png'}]});return
        self.reply({})
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers.get('Content-Length',0))));requests.append(body)
        tools=body.get('tools') and not any(m['role']=='tool' for m in body['messages'])
        msg={'role':'assistant','content':'Fixture answer with sources.'}
        if tools:msg.update(content=None,tool_calls=[{'id':'fixture-search','type':'function','function':{'name':'web_search','arguments':json.dumps({'query':'fixture current image'})}}])
        self.reply({'choices':[{'message':msg}]})
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);base='http://127.0.0.1:'+str(server.server_port)
threading.Thread(target=server.serve_forever,daemon=True).start()
def run(name,settings=None,flags=(),seconds=4):
    requests.clear();folder=root/name;profile=folder/'profile';profile.mkdir(parents=True,exist_ok=True)
    config={'encrypt_index':False,'roots':[],'include_programs':False,'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'language':'en','sidebar_collapsed':True,'default_search_mode':'Names','llm_enabled':True,'llm_vision':True,'llm_url':base+'/v1','llm_model':'fixture','llm_context':8192,'llm_response':'Full','clipboard_enabled':False,'web':{'enabled':True,'searxng_url':base,'previews':True}}
    config.update(settings or {});(profile/'settings.json').write_text(json.dumps(config))
    command=[str(pathlib.Path(args.exe).resolve()),'--smoke-report',str(folder/'report.json'),'--smoke-screenshot',str(folder/'view.png'),'--smoke-seconds',str(seconds),'--smoke-keep-open',*flags]
    result=subprocess.run(command,env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=25)
    assert result.returncode==0,(name,result.returncode)
    report=json.loads((folder/'report.json').read_text())
    print(name, 'PASS',flush=True)
    return report,list(requests)
try:
    r,_=run('clipboard-full-width',flags=('--smoke-page','9'))
    available,group=r['clipboard_settings_width'];assert abs(available-group)<3,(available,group)
    assert available>r['window_size'][0]-150,r['window_size']
    r,_=run('search-default-settings',flags=('--smoke-page','4'))
    r,_=run('web-long-thumbnails',flags=('--smoke-tool','Web','--smoke-query','fixture','--smoke-web-enter'))
    text,thumb=r['web_columns'];assert thumb and text[2]+11<=thumb[0],(text,thumb)
    r,_=run('web-default-on',{'ai_web_default':True},('--smoke-tool','AI'))
    assert r['ai_web_enabled'] and not requests
    r,_=run('web-disabled',{'ai_web_default':True,'web':{'enabled':False,'searxng_url':base}},('--smoke-tool','AI'))
    assert not r['ai_web_enabled']
    r,_=run('capture-defaults',{'ai_capture_web':True,'ai_capture_response':'Medium'},('--smoke-capture-hidden','--smoke-capture-fixture',str(fixture)))
    assert r['manager']['capture_attached'] and r['ai_web_enabled'] and r['response_length']=='Medium' and r['capture_defaults_pending'],r
    r,bodies=run('capture-submit-restores',{'ai_capture_web':True,'ai_capture_response':'Medium'},('--smoke-shortcut','capture_ai','--smoke-capture-fixture',str(fixture),'--smoke-submit-capture'))
    assert r['ai_answer']=='Fixture answer with sources.',r
    assert not r['ai_web_enabled'] and r['response_length']=='Full' and not r['capture_defaults_pending'],r
    assert bodies and bodies[0].get('tools'),bodies
    assert '8' in bodies[0]['messages'][0]['content'] and '12' in bodies[0]['messages'][0]['content'],bodies[0]
    assert any(isinstance(m.get('content'),list) and any(v.get('type')=='image_url' for v in m['content']) for m in bodies[0]['messages'])
    r,_=run('capture-web-off',{'ai_web_default':True,'ai_capture_web':False,'ai_capture_response':'Brief'},('--smoke-shortcut','capture_ai','--smoke-capture-fixture',str(fixture)))
    assert not r['ai_web_enabled'] and r['response_length']=='Brief' and r['capture_defaults_pending']
    r,bodies=run('capture-submit-web-off',{'ai_web_default':True,'ai_capture_web':False,'ai_capture_response':'Brief'},('--smoke-shortcut','capture_ai','--smoke-capture-fixture',str(fixture),'--smoke-submit-capture'))
    assert r['ai_answer']=='Fixture answer with sources.' and r['ai_web_enabled'] and r['response_length']=='Full' and not r['capture_defaults_pending'],r
    assert bodies and not bodies[0].get('tools'),bodies
    for quick in [False, True]:
        flags=('--smoke-tool','AI','--smoke-manual-area','--smoke-capture-fixture',str(fixture))+ (('--smoke-quick',) if quick else ())
        r,_=run('manual-area-popup' if quick else 'manual-area-main',flags=flags)
        assert r['manager']['capture_attached'] and r['window_visible'] and r['quick']==quick and r['query_focus'],r
        assert not r['manager']['capture_busy'] and not r['manager']['capture_open'],r
        flags=('--smoke-tool','AI','--smoke-manual-area','--smoke-capture-fixture',str(root/'missing.png'))+ (('--smoke-quick',) if quick else ())
        r,_=run('manual-area-error-popup' if quick else 'manual-area-error-main',flags=flags)
        assert r['window_visible'] and r['quick']==quick and r['ai_error'] and not r['manager']['capture_busy'],r
    for tool in ['Search','Clipboard','AI']:
        r,_=run('drop-'+tool.lower(),{'image_search_auto':False,'clipboard_enabled':True},('--smoke-tool',tool,'--smoke-drop-image',str(fixture)))
        assert r['tool']==tool,r
        assert r['manager']['capture_attached'] if tool=='AI' else r['image_query_ready'],r
        if tool=='AI':assert r['query_focus'] and not r['ai_busy'],r
    r,bodies=run('drop-ai-submit',flags=('--smoke-tool','AI','--smoke-drop-image',str(fixture),'--smoke-followup','Describe the dropped image'))
    assert r['ai_answer']=='Fixture answer with sources.' and bodies,r
    assert any(isinstance(m.get('content'),list) and any(v.get('type')=='image_url' for v in m['content']) for m in bodies[0]['messages']),bodies
    invalid=root/'invalid.png';invalid.write_bytes(b'not an image')
    r,_=run('drop-invalid-ai',flags=('--smoke-tool','AI','--smoke-drop-image',str(invalid)),seconds=2)
    assert r['ai_error'] and not r['manager']['capture_attached'],r
    print('PASS: 18 GUI cases including delayed hidden-window capture, original window mode, image drops and decode errors',flush=True)
finally:server.shutdown()
