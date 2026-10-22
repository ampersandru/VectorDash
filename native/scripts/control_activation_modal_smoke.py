"""Disposable native activation and owned-modal recovery. No user apps/files."""
import argparse,json,pathlib,queue,subprocess,sys,threading,time,uuid
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/debug/vectordash.exe');a=p.parse_args()
root=pathlib.Path('windows/.smoke/activation-modal').resolve()/uuid.uuid4().hex;root.mkdir(parents=True)
state=root/'state.txt';ready=root/'ready.json';title='VectorDash activation fixture '+uuid.uuid4().hex[:8]
fixture=subprocess.Popen([sys.executable,str(pathlib.Path(__file__).with_name('control_fixture.py')),str(state),str(ready),title,'--activation'],stdout=subprocess.DEVNULL,stderr=(root/'fixture.log').open('w'))
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
 def call(action,_reject=False,**args):
  worker.stdin.write(json.dumps({'tool':'computer_use','args':{'action':action,**args},'vision':False,'owner_pid':0})+'\n');worker.stdin.flush()
  v=json.loads(responses.get(timeout=25))
  with (root/'trace.jsonl').open('a',encoding='utf-8') as log:log.write(json.dumps({'action':action,'result':v})+'\n')
  assert ('error' in v) if _reject else ('error' not in v),v
  return v
 def click(snap,name,**args):
  rows=[r for r in snap['elements'] if r.get('name')==name];assert len(rows)==1,(name,rows)
  return call('click',window_id=snap['window_id'],snapshot_id=snap['snapshot_id'],element=rows[0]['id'],**args)
 wid=next(w['window_id']for w in call('windows')['windows']if w['title']==title)
 snap=call('inspect',window_id=wid)
 snap=click(snap,'Fixture Archive')
 assert not state.exists(),'Selection unexpectedly activated item'
 stale=snap.copy()
 snap=click(snap,'Fixture Archive',activate=True)
 assert state.read_text(encoding='utf-8')=='activated Fixture Archive'
 assert snap['action_method']=='activate_enter' and 'snapshot_id' in snap,snap
 # Reject stale snapshot even for explicit activation, without replaying Enter.
 item=next(r for r in stale['elements'] if r.get('name')=='Fixture Archive')
 rejected=call('click',_reject=True,window_id=wid,snapshot_id=stale['snapshot_id'],element=item['id'],activate=True)
 assert 'stale' in rejected['error'].lower(),rejected
 snap=call('inspect',window_id=wid)
 snap=click(snap,'Open dialog')
 assert snap['modal_dialog'] and snap['modal_transition']=='opened',snap
 # Address dialog directly, as models sometimes do after listing windows.
 wins=call('windows')['windows'];dialog=next(w['window_id']for w in wins if w['title']=='VectorDash modal fixture')
 snap=call('inspect',window_id=dialog)
 saves=[r for r in snap['elements']if r.get('name')=='Save'];assert len(saves)==2
 snap=call('click',window_id=dialog,snapshot_id=snap['snapshot_id'],element=saves[1]['id'])
 assert snap.get('modal_transition')=='closed' and snap.get('title')==title and 'snapshot_id'in snap,snap
 assert state.read_text(encoding='utf-8')=='modal second clicked'
 print('PASS selection vs activation, fresh activation observation, modal opened, direct-dialog close recovers parent without replay',flush=True)
 print('Artifacts:',root,flush=True)
finally:
 if worker:worker.terminate();worker.wait()
 if fixture.poll() is None:fixture.terminate()
 fixture.wait()
