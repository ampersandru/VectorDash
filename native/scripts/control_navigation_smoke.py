"""Native traversal and nested-scroll regressions."""
import argparse,json,pathlib,queue,statistics,subprocess,sys,threading,time,uuid
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');a=p.parse_args()
root=pathlib.Path('windows/.smoke/control-navigation').resolve()/uuid.uuid4().hex;root.mkdir(parents=True)
state=root/'state.txt';ready=root/'ready.json';title='VectorDash navigation fixture '+uuid.uuid4().hex[:8]
fixture=subprocess.Popen([sys.executable,str(pathlib.Path(__file__).with_name('control_fixture.py')),str(state),str(ready),title,'--navigation'],stdout=subprocess.DEVNULL,stderr=(root/'fixture.log').open('w'))
worker=None
try:
 deadline=time.monotonic()+8
 while not ready.exists() and time.monotonic()<deadline:
  assert fixture.poll() is None,(root/'fixture.log').read_text();time.sleep(.05)
 assert ready.exists()
 worker=subprocess.Popen([str(pathlib.Path(a.exe).resolve()),'--control-worker'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,encoding='utf-8')
 responses=queue.Queue()
 def reader():
  for line in worker.stdout:responses.put(line)
 threading.Thread(target=reader,daemon=True).start()
 timings={}
 def call(action,**args):
  started=time.perf_counter()
  worker.stdin.write(json.dumps({'tool':'computer_use','args':{'action':action,**args},'vision':True,'owner_pid':0})+'\n');worker.stdin.flush();result=json.loads(responses.get(timeout=25));timings.setdefault(action,[]).append((time.perf_counter()-started)*1000);return result
 def good(v):assert 'error' not in v,v;return v
 snap=good(call('windows'));wid=next(v['window_id'] for v in snap['windows'] if v['title']==title)
 pages=[];offset=0;snap=None
 for _ in range(35):
  snap=good(call('inspect',window_id=wid,offset=offset));pages.extend(snap['elements'])
  late=next((v for v in snap['elements'] if v.get('name')=='Late control 239'),None)
  if late:break
  next_offset=snap.get('next_offset');assert next_offset is not None and next_offset>offset,(offset,{k:v for k,v in snap.items() if k!='elements'})
  offset=next_offset
 assert late and late['id']>160,(offset,len(pages))
 # Only elements on the returned page may be acted on.
 stale_page=call('click',window_id=wid,snapshot_id=snap['snapshot_id'],element=0)
 assert 'error' in stale_page,stale_page
 snap=good(call('click',window_id=wid,snapshot_id=snap['snapshot_id'],element=late['id']))
 assert state.read_text(encoding='utf-8')=='late clicked 739'
 print('PASS true pagination beyond 160 elements, late-control invocation, unobserved-page rejection',flush=True)
 item=next(v for v in snap['elements'] if v.get('name')=='Scroll item 0')
 snap=good(call('scroll',window_id=wid,snapshot_id=snap['snapshot_id'],element=item['id'],delta=120))
 r=snap['scroll_result'];assert r['method']=='uia' and r['moved'] and r['after_percent']>r['before_percent'],r
 small_distance=r['after_percent']-r['before_percent']
 snap=good(call('scroll',window_id=wid,snapshot_id=snap['snapshot_id'],delta=600));r=snap['scroll_result'];assert r['moved'] and r['after_percent']-r['before_percent']>small_distance,r
 for _ in range(15):
  snap=good(call('scroll',window_id=wid,snapshot_id=snap['snapshot_id'],delta=3000))
  if snap['scroll_result']['moved']==False:break
 assert snap['scroll_result']['moved']==False and 'progress_warning' in snap,snap.get('scroll_result')
 print('PASS scrollable ancestor and default pane resolution, magnitude/direction and measured boundary',flush=True)
 snap=good(call('screenshot',window_id=wid));b=next(v for v in snap['elements'] if v.get('name')=='Apply');r=b['bounds'];ib=b['image_bounds'];wb=snap['window_bounds'];sx=snap['image_width']/(wb[2]-wb[0]);sy=snap['image_height']/(wb[3]-wb[1])
 assert abs(ib[0]-(r[0]-wb[0])*sx)<.06 and abs(ib[1]-(r[1]-wb[1])*sy)<.06
 print('PASS screenshot-space element bounds',flush=True)
 summary={k:{'calls':len(v),'median_ms':round(statistics.median(v),1),'max_ms':round(max(v),1)} for k,v in timings.items()}
 (root/'timings.json').write_text(json.dumps(summary,indent=2),encoding='utf-8');print(json.dumps(summary),flush=True)
 print('Artifacts:',root,flush=True)
finally:
 if worker:worker.terminate();worker.wait()
 fixture.terminate();fixture.wait()
