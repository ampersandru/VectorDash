"""Synthetic search benchmark; excludes model and UI latency."""
import json,random,time,statistics,sys
from pathlib import Path
from unittest.mock import patch
from engine import Engine,DEFAULTS,model_signature
home=Path(__file__).parent/'.smoke/search-benchmark';home.mkdir(parents=True,exist_ok=True)
model=home/'model.gguf';model.write_bytes(b'fixture')
settings=dict(DEFAULTS,semantic=True,model=str(model),ocr_enabled=False)
engine=Engine(home)
r=random.Random(43)
with engine.connect() as db:
 if not db.execute('SELECT 1 FROM segments LIMIT 1').fetchone():
  for i in range(1500):
   vector=[r.random() for _ in range(768)]
   db.execute('INSERT INTO segments VALUES(?,?,?,?,?,?)',(f'C:/photos/dog_{i}.jpg','files','image','photo',json.dumps(vector),'{}'))
  db.execute('INSERT INTO words(rowid,path,text) SELECT rowid,path,text FROM segments')
 db.execute("INSERT OR REPLACE INTO metadata VALUES('signature',?)",(model_signature(settings),))
with patch('engine.verify_server_model'),patch('engine.embed',return_value=[.5]*768):
 timings=[]
 for query in ('images of dogs','photos of dogs','a dog on the beach','images of dogs'):
  start=time.perf_counter();engine.search(query,settings);timings.append((time.perf_counter()-start)*1000)
 print(json.dumps({'files':1500,'segments':1500,'dimension':768,'ms':timings,'warm_median_ms':statistics.median(timings[1:])}))
