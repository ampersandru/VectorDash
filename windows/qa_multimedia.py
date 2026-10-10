"""Opt-in real model QA. Requires the isolated server on port 8788 and local fixtures."""
import os
os.environ['VECTORDASH_SERVER_URL']='http://127.0.0.1:8788'
import json
from pathlib import Path
import subprocess
import sys
sys.stdout.reconfigure(encoding="utf-8")
import pymupdf
from engine import Engine,DEFAULTS
from content import ffmpeg

root=Path(__file__).parent.resolve()/'.smoke/multimedia'
source=root/'source'
project=root/'project'
project.mkdir(exist_ok=True)
if not (project/'.git').exists():
    subprocess.run(['git','init',str(project)],check=True,capture_output=True,creationflags=subprocess.CREATE_NO_WINDOW)
source.mkdir(exist_ok=True)
managed=Path(__file__).parent.resolve()/'.smoke/managed'
info=json.loads((managed/'result.json').read_text())
settings=dict(DEFAULTS,roots=[str(source)],semantic=True,model=info['model'],projector=str(managed/'models/mmproj-embeddinggemma-2-Q8_0.gguf'),code_enabled=True,code_roots=[str(project)])
flags={'check':True,'capture_output':True,'creationflags':subprocess.CREATE_NO_WINDOW}
if not (source/'004.mp4').exists():
    subprocess.run([ffmpeg(settings),'-y','-loop','1','-i',str(root/'blank.png'),'-loop','1','-i',str(root/'scene.png'),'-filter_complex','[0:v]trim=duration=6,setpts=PTS-STARTPTS[a];[1:v]trim=duration=6,setpts=PTS-STARTPTS[b];[a][b]concat=n=2:v=1:a=0[out]','-map','[out]','-r','10','-c:v','libx264','-pix_fmt','yuv420p',str(source/'004.mp4')],**flags)
if not (source/'005.wav').exists():
    subprocess.run([ffmpeg(settings),'-y','-i',str(root/'speech.wav'),'-af','adelay=32000:all=1','-ac','1','-ar','16000',str(source/'005.wav')],**flags)
(source/'006.txt').write_text('ঘুম ভালো হলে শরীর ও মন সুস্থ থাকে। প্রতিদিন নিয়মিত সময়ে ঘুমানো উচিত।',encoding='utf-8')
(source/'007.txt').write_text('يجب شرب الماء بانتظام للحفاظ على صحة الجسم وتجنب الجفاف.',encoding='utf-8')
(source/'008.txt').write_text('Healthy sleep improves memory and helps the body recover overnight.',encoding='utf-8')
with pymupdf.open() as doc:
    page=doc.new_page(); page.insert_text((72,72),'Lease agreement. Rent is due on the first day of each month.')
    page=doc.new_page(); page.insert_text((72,72),'Pets clause: Dogs and cats are permitted with written landlord approval.')
    doc.save(source/'009.pdf')
(project/'retry.py').write_text('import time\n\ndef retry_request(call, attempts=4):\n    """Retry a failed network call with exponential backoff."""\n    for attempt in range(attempts):\n        try:\n            return call()\n        except ConnectionError:\n            if attempt == attempts - 1:\n                raise\n            time.sleep(2 ** attempt)\n',encoding='utf-8')
engine=Engine(root)
print(engine.index(settings,progress=lambda text:print(text,flush=True),force=True),flush=True)
queries=['zebra in a video','where they talk about sleep','the clause about pets in the lease','a dog','a dog on the beach','cellphone','payment declined error','INV-2026-00891','E_PAYMENT_402','regular bedtime','drink water to avoid dehydration','ঘুম শরীর সুস্থ রাখে','code: retry a failed call with backoff']
report={}
for query in queries:
    results=engine.search(query,settings)
    report[query]=[{'file':Path(r.path).name,'score':round(r.score,4),'kind':r.kind,'page':r.page,'seconds':r.seconds,'line':r.line,'snippet':r.snippet} for r in results[:5]]
    print(query,json.dumps(report[query],ensure_ascii=False),flush=True)
(root/'report.json').write_text(json.dumps(report,indent=2,ensure_ascii=False),encoding='utf-8')
assert report['zebra in a video'][0]['file']=='004.mp4' and report['zebra in a video'][0]['seconds']>=6
assert any(r['file']=='005.wav' and r['seconds']>=25 for r in report['where they talk about sleep'])
assert report['the clause about pets in the lease'][0]['page']==2
assert report['a dog'][0]['file'] in ('002.jpg','011.jpg')
assert report['a dog on the beach'][0]['file']=='011.jpg'
assert report['cellphone'][0]['file']=='010.jpg'
assert report['payment declined error'][0]['file']=='003.png'
assert report['INV-2026-00891'][0]['file']=='003.png'
assert report['E_PAYMENT_402'][0]['file']=='003.png'
assert report['drink water to avoid dehydration'][0]['file']=='007.txt'
assert any(r['file']=='008.txt' for r in report['ঘুম শরীর সুস্থ রাখে'])
assert report['code: retry a failed call with backoff'][0]['line']==3
print('Real multimodal retrieval checks passed.',flush=True)
