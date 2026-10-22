"""Focused editable control outside the traversal page. No user apps/files."""
import argparse,json,pathlib,queue,subprocess,sys,threading,time,uuid
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');a=p.parse_args()
root=pathlib.Path('windows/.smoke/focused-pagination').resolve()/uuid.uuid4().hex;root.mkdir(parents=True)
state=root/'state.txt';ready=root/'ready.json';title='VectorDash pagination fixture '+uuid.uuid4().hex[:8]
fixture=subprocess.Popen([sys.executable,str(pathlib.Path(__file__).with_name('control_fixture.py')),str(state),str(ready),title,'--late-focus'],stdout=subprocess.DEVNULL,stderr=(root/'fixture.log').open('w'))
worker=None
try:
 deadline=time.monotonic()+10
 while not ready.exists() and time.monotonic()<deadline:
  assert fixture.poll() is None,(root/'fixture.log').read_text();time.sleep(.05)
 assert ready.exists()
 worker=subprocess.Popen([str(pathlib.Path(a.exe).resolve()),'--control-worker'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,encoding='utf-8')
 responses=queue.Queue()
 def reader():
  for line in worker.stdout:responses.put(line)
 threading.Thread(target=reader,daemon=True).start()
 def call(action,_reject=False,_allow_focus_blocked=False,**args):
  worker.stdin.write(json.dumps({'tool':'computer_use','args':{'action':action,**args},'vision':False,'owner_pid':0})+'\n');worker.stdin.flush()
  v=json.loads(responses.get(timeout=25))
  with (root/'trace.jsonl').open('a',encoding='utf-8') as log:log.write(json.dumps({'action':action,'result':v})+'\n')
  if _allow_focus_blocked and 'Windows blocked foreground input' in v.get('error',''):return v
  assert ('error' in v) if _reject else ('error' not in v),v
  return v
 def click(snap,name,**args):
  rows=[r for r in snap['elements'] if r.get('name')==name]
  for _ in range(12):
   if rows or snap.get('next_offset') is None:break
   snap=call('inspect',window_id=snap['window_id'],offset=snap['next_offset']);rows=[r for r in snap['elements']if r.get('name')==name]
  assert len(rows)==1,(name,rows)
  return call('click',window_id=snap['window_id'],snapshot_id=snap['snapshot_id'],element=rows[0]['id'],element_name=name,**args)
 wid=next(w['window_id']for w in call('windows')['windows']if w['title']==title)
 snap=call('inspect',window_id=wid,offset=0)
 focused=[r for r in snap['elements']if r.get('focused') and r.get('pagination_independent')]
 if not focused:
  focus=call('focus',window_id=wid,_allow_focus_blocked=True)
  if 'error'in focus:
   print('SKIP focused pagination: Windows blocked foreground acquisition; no input sent',flush=True);sys.exit(0)
  snap=call('inspect',window_id=wid,offset=0)
  focused=[r for r in snap['elements']if r.get('focused') and r.get('pagination_independent')]
 assert len(focused)==1 and focused[0]['id']==4096,snap
 assert snap['offset']==0 and snap['next_offset'] is not None,snap
 snap=call('type',window_id=wid,snapshot_id=snap['snapshot_id'],element=4096,text='Pagination safe',text_mode='replace_all')
 focused=[r for r in snap['elements']if r.get('focused')]
 assert any(r.get('value')=='Pagination safe'for r in focused),snap
 print('PASS focused editor outside first traversal page observed and typed with snapshot identity',flush=True)
 print('Artifacts:',root,flush=True)

finally:
 if worker:worker.terminate();worker.wait()
 if fixture.poll() is None:fixture.terminate()
 fixture.wait()
