# Run from the repository root after building the Windows app.
import argparse, json, os, pathlib, subprocess, threading, time, urllib.parse
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
parser = argparse.ArgumentParser(description='Windows Web search GUI checks against a local mock; no API keys required.')
parser.add_argument('--exe', default='native/target/debug/vectordash.exe')
options = parser.parse_args()
root = pathlib.Path('windows/.smoke/web-ui').resolve()
root.mkdir(parents=True, exist_ok=True)
requests = []

class Handler(BaseHTTPRequestHandler):

    def log_message(self, *args):
        pass

    def do_GET(self):
        p = urllib.parse.urlsplit(self.path)
        if p.path.startswith('/thumb'):
            import io
            from PIL import Image
            out=io.BytesIO(); Image.new('RGB',(320,180),(50,150,200)).save(out,format='PNG'); data=out.getvalue()
            self.send_response(200); self.send_header('Content-Length',str(len(data))); self.end_headers(); self.wfile.write(data); return
        q = urllib.parse.parse_qs(p.query).get('q', [''])[0]
        requests.append((p.path, q))
        if p.path.startswith('/slow'):
            time.sleep(4)
        if p.path.startswith('/deny'):
            self.send_response(403)
            self.end_headers()
            return
        data = json.dumps({'results': [{'title': 'Web result ' + str(i), 'url': 'https://example.org/' + str(i), 'snippet': 'A useful web result, returned without loading an embedding model.', 'source': 'Fixture', 'thumbnail': base + '/thumb'+str(i)+'.png'} for i in range(8)]}).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        try:
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
            pass
server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
base = 'http://127.0.0.1:' + str(server.server_port)

def run(name, flags=(), seconds=5, enabled=True, slow=False):
    folder = root / name
    profile = folder / 'profile'
    profile.mkdir(parents=True, exist_ok=True)
    settings = {'encrypt_index': False, 'roots': [], 'include_programs': False, 'watch_changes': False, 'mcp_enabled': False, 'auto_check_updates': False, 'default_search_mode': 'Names', 'embedding_server': 'Remote', 'remote_url': base + '/embeddings/v1', 'remote_model': 'fixture', 'llm_enabled': True, 'llm_url': base + '/llm/v1', 'llm_model': 'fixture', 'clipboard_enabled': True, 'web': {'enabled': enabled, 'provider': 'SearXNG', 'searxng_url': base + ('/slow' if slow else '/searx'), 'degoog_url': base + '/degoog', 'brave_key': 'fixture'}}
    settings['web']['previews'] = name == 'previews'
    if name == 'reordered':
        settings['search_tool_order'] = ['Web', 'AI', 'Search', 'Clipboard']
    (profile / 'settings.json').write_text(json.dumps(settings))
    args = [str(pathlib.Path(options.exe).resolve()), '--smoke-report', str(folder / 'report.json'), '--smoke-screenshot', str(folder / 'view.png'), '--smoke-seconds', str(seconds), '--smoke-tool', 'Web', '--smoke-query', name, '--smoke-keep-open', *flags]
    if name in ['outside-after', 'click-result']:
        args = args[:args.index('--smoke-screenshot')] + args[args.index('--smoke-screenshot') + 2:]
    p = subprocess.run(args, env={**os.environ, 'VECTORDASH_DATA_HOME': str(profile)}, timeout=seconds + 15, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    r = json.loads((folder / 'report.json').read_text())
    w = r['web']
    calls = [a for a in requests if a[1] == name]
    assert not r.get('embedding_activity'), name
    print(json.dumps({'case': name, 'exit': p.returncode, 'tool': r.get('tool'), 'provider': w['provider'], 'busy': w['busy'], 'results': len(w['results']), 'visible': w['window_visible'], 'opened': w['opened'], 'outside_during': w['outside_during'], 'outside_after': w['outside_after'], 'size': r.get('window_size'), 'menu_last': w.get('provider_menu_last'), 'requests': calls}), flush=True)
    return (r, calls)
r, c = run('typing', ('--smoke-quick',))
assert not c and (not r['web']['results'])
r, c = run('main-submit', ('--smoke-web-enter',))
assert len(c) == 1 and len(r['web']['results']) == 8
r, c = run('provider', ('--smoke-quick', '--smoke-web-provider-picker', '--smoke-web-pick', 'Degoog', '--smoke-web-enter'))
assert len(c) == 1 and c[0][0] == '/degoog/api/search' and (r['web']['provider'] == 'Degoog')
r, c = run('menu', ('--smoke-quick', '--smoke-web-provider-picker'))
assert r['web']['provider_menu_last'] and r['web']['provider_menu_last'][3] < r['window_size'][1]
r, c = run('busy-outside', ('--smoke-quick', '--smoke-web-enter', '--smoke-web-outside-during'), seconds=3, slow=True)
assert r['web']['busy'] and r['web']['outside_during'] and r['web']['window_visible']
r, c = run('finish-after-outside', ('--smoke-quick', '--smoke-web-enter', '--smoke-web-outside-during'), seconds=7, slow=True)
assert not r['web']['busy'] and r['web']['outside_during'] and r['web']['window_visible']
r, c = run('outside-after', ('--smoke-quick', '--smoke-web-enter', '--smoke-web-outside-after'))
assert r['web']['outside_after'] and (not r['web']['window_visible'])
r, c = run('click-result', ('--smoke-quick', '--smoke-web-enter', '--smoke-web-open', '--smoke-web-no-browser'))
assert r['web']['opened'] == 'https://example.org/0' and (not r['web']['window_visible'])
r, c = run('disabled', ('--smoke-quick',), enabled=False)
assert r['tool'] == 'Search' and 'Web' not in r['tools'] and (not c)
r, c = run('settings', ('--smoke-page', '8'))
assert not c
r, c = run('reordered', ('--smoke-quick',))
assert r['tools'] == ['Web', 'AI', 'Search', 'Clipboard'] and (not c)
print('PASS: 11 Web UI scenarios', flush=True)

r,c=run('previews',('--smoke-quick','--smoke-web-enter'))
assert r['web_thumbnail_count']>0,r
print('PASS: optional provider thumbnails loaded',flush=True)

server.shutdown()
