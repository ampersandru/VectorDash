"""Isolated GUI checks for working-trail colors and monitor position restoration."""
import argparse,json,os,pathlib,subprocess
from PIL import Image
p=argparse.ArgumentParser();p.add_argument('--exe',default='native/target/release/vectordash.exe');args=p.parse_args()
root=pathlib.Path('windows/.smoke/preview-trails').resolve()
cases=[('appearance','Search','--smoke-trail-settings',None),('search','Search','--smoke-embedding-processing',[20,245,65]),('clipboard','Clipboard','--smoke-embedding-processing',[245,160,20]),('ai','AI','--smoke-ai-processing',[20,225,245]),('microphone','Search','--smoke-recording',[245,20,225]),('monitors','Search','--smoke-move-monitors',None)]
for name,tool,flag,color in cases:
 folder=root/name;profile=folder/'profile';profile.mkdir(parents=True,exist_ok=True)
 settings={'language':'en','sidebar_collapsed':True,'encrypt_index':False,'roots':[],'watch_changes':False,'mcp_enabled':False,'auto_check_updates':False,'include_programs':False,'default_search_mode':'Names','clipboard_enabled':True,'llm_enabled':True,'llm_url':'','llm_model':'fixture','trail_colors':{'Search':[[20,245,65]]*2,'Clipboard':[[245,160,20]]*2,'AI':[[20,225,245]]*2,'Microphone':[[245,20,225]]*2}}
 (profile/'settings.json').write_text(json.dumps(settings))
 flags=['--smoke-page','4',flag] if name=='appearance' else ['--smoke-tool',tool,flag]
 if name=='monitors':flags+=['--smoke-quick']
 seconds='7' if name=='monitors' else '3'
 command=[str(pathlib.Path(args.exe).resolve()),'--smoke-report',str(folder/'report.json'),'--smoke-screenshot',str(folder/'view.png'),'--smoke-seconds',seconds,'--smoke-keep-open',*flags]
 run=subprocess.run(command,env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=25)
 assert run.returncode==0,(name,run.returncode)
 r=json.loads((folder/'report.json').read_text())
 if color:
  img=Image.open(folder/'view.png').convert('RGB');w,h=img.size
  pixels=[img.getpixel((x,y)) for x in range(w) for y in (2,3,4,h-5,h-4,h-3)]
  pixels.extend(img.getpixel((x,y)) for x in (2,3,4,w-5,w-4,w-3) for y in range(h))
  assert sum(max(abs(a-b) for a,b in zip(pixel,color))<55 for pixel in pixels)>15,(name,'Custom border color not rendered')
 if name=='monitors':
  assert r['monitor_moves'],'No monitors tested'
  for requested,actual in r['monitor_moves']:assert max(abs(a-b) for a,b in zip(requested,actual))<=8,(requested,actual)
 print('PASS',name,r.get('monitor_moves') if name=='monitors' else '',flush=True)
print('PASS: working-trail colors, settings layout and physical monitor positioning',flush=True)
