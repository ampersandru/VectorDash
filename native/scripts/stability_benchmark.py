"""Isolated before/after search and indexing benchmarks. Never opens the user's index.
Requires Pillow and psutil in the development Python environment.
Optional --live-settings reads embedding configuration only; generated documents are
sent to that configured model server. Credentials are never written to reports.
"""
import argparse
import base64
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import math
import os
from pathlib import Path
import shutil
import sqlite3
import socket
import statistics
import struct
import subprocess
import tempfile
import threading
import time
from PIL import Image, ImageDraw
import psutil

parser=argparse.ArgumentParser()
parser.add_argument('--exe',required=True)
parser.add_argument('--baseline',required=True)
parser.add_argument('--output',required=True)
parser.add_argument('--live-settings')
parser.add_argument('--catalog-size',type=int,default=10000)
args=parser.parse_args()
root=Path(tempfile.mkdtemp(prefix='VectorDash-stability-bench-'))
output=Path(args.output);output.parent.mkdir(parents=True,exist_ok=True)
executables={'before':Path(args.baseline).resolve(),'after':Path(args.exe).resolve()}
report={'fixture_root':str(root),'catalog_files':args.catalog_size,'search':[],'indexing':[],'pdf':[],'live':{},'limitations':['Generated corpus, not a broad retrieval-quality benchmark.','Process-tree RSS excludes VRAM.','Search timings include one cold query followed by warm queries; OS disk caches are not flushed.','Mock-server indexing measures pipeline overhead plus a fixed 5ms service delay, not model inference.']}
requests=[]
class Handler(BaseHTTPRequestHandler):
    protocol_version='HTTP/1.1'
    def setup(self):
        super().setup();self.connection.setsockopt(socket.IPPROTO_TCP,socket.TCP_NODELAY,1)
    def do_POST(self):
        value=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        items=value.get('input',[]);items=items if isinstance(items,list) else [items]
        requests.append(len(items));time.sleep(.005)
        vectors=[]
        for i,item in enumerate(items):
            text=str(item).lower();target='needle' in text
            vector=[0.0]*768;vector[0 if target else 1]=1.0
            vectors.append({'index':i,'embedding':vector})
        body=json.dumps({'data':vectors}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    def log_message(self,*args):pass
server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
threading.Thread(target=server.serve_forever,daemon=True).start()
mock={'encrypt_index':False,'embedding_server':'Remote','remote_url':f'http://127.0.0.1:{server.server_port}/v1','remote_model':'fixture','include_programs':False,'watch_changes':False,'auto_check_updates':False,'check_cpu':False,'check_gpu':False,'check_user_idle':False,'ac_only':False,'defer_search_model':False,'pause_for_game':False,'pause_for_fullscreen':False,'batch_size':4}

def settings(home,values):
    home.mkdir(parents=True,exist_ok=True)
    (home/'settings.json').write_text(json.dumps(values),encoding='utf-8')

def run(exe,home,*arguments,worker=None,timeout=180):
    env=os.environ.copy();env['VECTORDASH_DATA_HOME']=str(home)
    process=subprocess.Popen([str(exe),*arguments],env=env,stdin=subprocess.PIPE if worker is not None else subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8')
    peak=[0];cpu={};stop=threading.Event()
    def sample():
        while not stop.wait(.01):
            try:
                parent=psutil.Process(process.pid);rss=0
                for proc in [parent]+parent.children(recursive=True):
                    try:
                        rss+=proc.memory_info().rss;t=proc.cpu_times();cpu[proc.pid]=t.user+t.system
                    except psutil.Error:pass
                peak[0]=max(peak[0],rss)
            except psutil.Error:pass
    monitor=threading.Thread(target=sample);monitor.start();start=time.perf_counter()
    try:stdout,stderr=process.communicate(json.dumps(worker)+'\n' if worker is not None else None,timeout=timeout)
    except subprocess.TimeoutExpired:
        process.kill();process.communicate();raise
    finally:stop.set();monitor.join()
    if process.returncode:raise RuntimeError(stderr[-1000:])
    metrics={'wall_s':round(time.perf_counter()-start,4),'peak_rss_mib':round(peak[0]/1048576,1),'cpu_s':round(sum(cpu.values()),4)}
    if worker is not None:
        rows=[json.loads(line) for line in stdout.splitlines()];errors=[row['error'] for row in rows if 'error'in row]
        if errors:raise RuntimeError(str(errors))
        metrics['segments']=sum('segment'in row for row in rows)
        return None,metrics
    return json.loads(stdout),metrics

try:
    catalog=root/'catalog';catalog.mkdir();home=root/'search-home'
    values={**mock,'roots':[{'path':str(catalog),'mode':'full'}]};settings(home,values)
    run(executables['after'],home,'--query','','--mode','Names')
    c=sqlite3.connect(home/'index.sqlite');c.execute('BEGIN')
    location=json.dumps({'page':1,'seconds':-1,'line':0,'embedding_source':'text'})
    v0=struct.pack('<768f',1,*([0]*767));v1=struct.pack('<768f',0,1,*([0]*766))
    for i in range(args.catalog_size):
        name='needle_document.txt' if i==0 else f'item-{i:06}.txt';path=str(catalog/name);body='NEEDLE invoice record' if i==0 else 'ordinary unrelated document'
        c.execute('INSERT INTO names VALUES(?,?,?,?,?,?,?,?,?)',(path,name,str(catalog),'file',path.lower(),name.lower(),100,2048,'full'))
        c.execute('INSERT INTO segments VALUES(?,?,?,?,?,?)',(path,'files','text',body,'[]',location));rid=c.execute('SELECT last_insert_rowid()').fetchone()[0]
        c.execute('INSERT INTO words(rowid,path,text) VALUES(?,?,?)',(rid,path,body))
        c.execute('INSERT INTO segment_vectors VALUES(?,?,?,?,?,?,?)',(rid,'files','text','text',768,v0 if i==0 else v1,path))
    c.execute("INSERT INTO name_words(name_words) VALUES('rebuild')");c.commit();c.close()
    for encrypted in [False,True]:
        if encrypted:run(executables['after'],home,'--convert-encryption','on')
        for profile in ['Low memory','Balanced','Fast search']:
            for mode in ['Names','Semantic','Both']:
                for label,exe in executables.items():
                    values=json.loads((home/'settings.json').read_text(encoding='utf-8'));values['performance']=profile;settings(home,values)
                    data,metrics=run(exe,home,'--query','needle','--mode',mode,'--repeat','12')
                    assert data['results'] and Path(data['results'][0]['path']).name=='needle_document.txt',(label,profile,mode)
                    times=data['search_times_ms'];warm=times[1:]
                    row={'build':label,'encrypted':encrypted,'profile':profile,'mode':mode,'cold_ms':round(times[0],3),'warm_median_ms':round(statistics.median(warm),3),'warm_p95_ms':round(sorted(warm)[math.ceil(len(warm)*.95)-1],3),**metrics};report['search'].append(row);print(json.dumps(row),flush=True)
    documents=root/'documents';documents.mkdir()
    for i in range(128):(documents/f'document-{i:03}.txt').write_text(('A short document about a garden and seasonal plants. '+str(i)+' ')*8,encoding='utf-8')
    for batch in [1,4,16]:
        for label,exe in executables.items():
            runs=[]
            for repeat in range(3):
                h=root/f'index-{label}-{batch}-{repeat}';settings(h,{**mock,'batch_size':batch});requests.clear()
                data,metrics=run(exe,h,'--index',str(documents),'--unencrypted')
                c=sqlite3.connect(h/'index.sqlite');count=c.execute("SELECT count(*) FROM native_content WHERE error='' ").fetchone()[0];c.close();assert count==128,(label,batch,count,data)
                runs.append({**metrics,'requests':len(requests),'files':count,'index_s':data['index_seconds']})
            row={'build':label,'batch_size':batch,'median_s':round(statistics.median(x['index_s'] for x in runs),4),'peak_rss_mib':max(x['peak_rss_mib']for x in runs),'median_requests':statistics.median(x['requests']for x in runs),'runs':runs};report['indexing'].append(row);print(json.dumps(row),flush=True)
    # Image-only PDF exercises the real packaged PDFium reader, without embedding inference.
    pdf=root/'pages.pdf';pages=[]
    for page in range(72):
        image=Image.effect_noise((640,480),48).convert('RGB');draw=ImageDraw.Draw(image);draw.rectangle((20,20,590,90),fill='white');draw.text((30,35),f'Page {page+1}: disposable PDF performance fixture',fill='black');pages.append(image)
    pages[0].save(pdf,save_all=True,append_images=pages[1:],resolution=96);pages.clear()
    for label,exe in executables.items():
        runs=[]
        for _ in range(3):
            _,metrics=run(exe,root,'--extract-worker',worker={'path':str(pdf),'settings':{**mock,'pdf_visuals':True,'ocr_enabled':False,'max_file_mib':128}});assert metrics['segments']==72; runs.append(metrics)
        row={'build':label,'pages':72,'median_s':round(statistics.median(x['wall_s']for x in runs),4),'peak_rss_mib':max(x['peak_rss_mib']for x in runs),'runs':runs};report['pdf'].append(row);print(json.dumps(row),flush=True)
    if args.live_settings:
        live=json.loads(Path(args.live_settings).read_text(encoding='utf-8'));live.update({k:v for k,v in mock.items()if k not in {'embedding_server','remote_url','remote_model'}})
        # Keep the configured remote URL/model/DPAPI-protected key, never include them in the report.
        original=json.loads(Path(args.live_settings).read_text(encoding='utf-8'))
        for key in ['embedding_server','remote_url','remote_model','remote_key','remote_context','remote_has_mmproj','model','mmproj','server','device','hardware','runtime_backend']: 
            if key in original:live[key]=original[key]
        live.update(index_audio=False,index_video=False,index_images=True,ocr_enabled=False,pdf_visuals=False)
        source=root/'live-documents';source.mkdir()
        topics=[('pets','A veterinarian explains puppy vaccinations, canine health and caring for a pet dog.','medical care for a young dog'),('car','An automobile mechanic replaces the motor oil and checks the engine brakes and tires.','vehicle maintenance at a repair garage'),('garden','Grow tomatoes in compost with daily watering and plenty of sunlight in the vegetable garden.','how to cultivate vegetables outside'),('travel','The airline boarding pass lists the flight gate, departure time, seat and airport terminal.','where to board an airplane'),('cooking','Bake a loaf by mixing flour, yeast and water, kneading the dough and heating it in an oven.','instructions for making homemade bread'),('finance','A monthly household budget lists rent, electricity bills, savings and total income.','planning personal expenses and earnings')]
        for topic,text,_ in topics:
            for i in range(4):(source/f'{topic}-{i}.txt').write_text(text,encoding='utf-8')
        for topic,color,query in [('visualred','red','a red square on a white background'),('visualblue','blue','a blue circle on a white background'),('visualgreen','green','a green triangle on a white background')]:
            image=Image.new('RGB',(512,512),'white');draw=ImageDraw.Draw(image)
            if color=='red':draw.rectangle((100,100,410,410),fill=color)
            elif color=='blue':draw.ellipse((100,100,410,410),fill=color)
            else:draw.polygon([(256,70),(70,440),(440,440)],fill=color)
            image.save(source/(topic+'-0.png'));topics.append((topic,'',query))
        live_data={};all_vectors={}
        for label,exe in executables.items():
            h=root/f'live-{label}';settings(h,live)
            indexed,metrics=run(exe,h,'--index',str(source),'--unencrypted',timeout=300)
            checks=[]
            for topic,_,query in topics:
                for mode in ['Semantic','Both']:
                    data,_=run(exe,h,'--query',query,'--mode',mode,timeout=120)
                    checks.append({'topic':topic,'mode':mode,'top1_correct':bool(data['results'])and Path(data['results'][0]['path']).stem.startswith(topic+'-'),'top5_correct':any(Path(hit['path']).stem.startswith(topic+'-')for hit in data['results'][:5]),'search_ms':round(data['search_ms'],3)})
            c=sqlite3.connect(h/'index.sqlite');vectors={Path(path).name:struct.unpack('<'+str(len(blob)//4)+'f',blob)for path,blob in c.execute('SELECT path,vector FROM segment_vectors')};c.close();all_vectors[label]=vectors
            live_data[label]={'index_s':indexed['index_seconds'],**metrics,'checks':checks,'top1':sum(x['top1_correct']for x in checks),'top5':sum(x['top5_correct']for x in checks),'queries':len(checks),'indexed_files':len(vectors)};print('LIVE',label,json.dumps(live_data[label]),flush=True)
        assert all_vectors['before'].keys()==all_vectors['after'].keys()
        cos=[]
        for key,a in all_vectors['before'].items():
            b=all_vectors['after'][key];assert len(a)==len(b)==768
            cos.append(sum(x*y for x,y in zip(a,b))/math.sqrt(sum(x*x for x in a)*sum(y*y for y in b)))
        report['live']={'results':live_data,'min_vector_cosine':min(cos),'mean_vector_cosine':statistics.mean(cos)}
        assert min(cos)>.999,'Batching changed document vectors'
        assert live_data['after']['top1']>=live_data['before']['top1'],'Retrieval accuracy regressed'
finally:
    output.write_text(json.dumps(report,indent=2),encoding='utf-8');server.shutdown()
    print('Report:',str(output),flush=True)
