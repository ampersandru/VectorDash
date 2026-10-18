# Run from the repository root after seeding the explicit clipboard UI fixture.

import os,json,pathlib,shutil,subprocess
root=pathlib.Path('windows/.smoke/clipboard-ui').resolve();base=root/'profile';exe=pathlib.Path('native/target/debug/vectordash.exe').resolve()
cases=[('clipboard-settings',['--smoke-page','9'],None),('shortcuts',['--smoke-page','4','--smoke-shortcuts-bottom'],None),('win-key',['--smoke-page','4','--smoke-win-bind'],None),('conflict',['--smoke-page','4','--smoke-shortcut-conflict'],None),('clipboard-mode',['--smoke-shortcut','clipboard'],'Clipboard'),('ai-mode',['--smoke-shortcut','ai'],'AI'),('web-mode',['--smoke-shortcut','web'],'Web'),('emoji-mode',['--smoke-shortcut','emoji'],'Emoji'),('area-ai',['--smoke-shortcut','capture_ai','--smoke-capture-fixture',str(root/'capture.png')],'AI'),('save-item',['--smoke-tool','Clipboard','--smoke-query','Ordinary','--smoke-pin-selected'],'Clipboard')]
cases += [('legacy-wide',['--smoke-tool','Clipboard','--smoke-query','Legacy wide fixture'],'Clipboard'),('hidden-capture',['--smoke-capture-hidden','--smoke-capture-fixture',str(root/'capture.png')],'AI'),('clipboard-cycle',['--smoke-tool','Clipboard','--smoke-cycle-file-type'],'Clipboard'),('search-cycle',['--smoke-quick','--smoke-tool','Search','--smoke-cycle-file-type'],'Search'),('image-turns',['--smoke-tool','AI','--smoke-ai-image-turns','--smoke-capture-fixture',str(root/'capture.png')],'AI')]

for name,flags,tool in cases:
 folder=root/name;profile=folder/'profile';profile.mkdir(parents=True,exist_ok=True)
 for filename in ['settings.json','clipboard.sqlite','index.key']:
  if (base/filename).exists():shutil.copyfile(base/filename,profile/filename)
 args=[str(exe),'--smoke-report',str(folder/'report.json'),'--smoke-screenshot',str(folder/'view.png'),'--smoke-seconds','4','--smoke-keep-open',*flags]
 proc=subprocess.run(args,env={**os.environ,'VECTORDASH_DATA_HOME':str(profile)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=18)
 r=json.loads((folder/'report.json').read_text());m=r['manager'];assert proc.returncode==0,name
 if tool:assert r['tool']==tool,(name,r['tool'])
 if name=='win-key':assert m['ai_binding']=='Ctrl+Win+K',m
 if name=='conflict':assert m['conflict']==['web','Ctrl+Alt+K','ai'],m
 if name=='legacy-wide':
  size=r['clipboard_texture_size'];assert size and abs(size[0]/size[1]-1108/351)<.02,size
 if name=='hidden-capture':assert r['main_visible'] and m['capture_attached'] and not m['capture_open'],m
 if name=='area-ai':assert m['capture_attached'] and not m['capture_busy'] and not m['capture_open'],m
 if name=='clipboard-cycle':assert m['clip_filter']=='Pinned items',m
 if name=='search-cycle':assert m['category']=='Images',m
 if name=='image-turns':assert m['ai_image_turns']==2 and m['ai_thumbnail_count']==2,m
 if name=='save-item':assert any(v.startswith('☆') and 'Ordinary' in v for v in m['clip_labels']),m
 print(json.dumps({'case':name,'tool':r['tool'],'manager':m}),flush=True)
print('PASS: 15 clipboard/shortcut UI scenarios',flush=True)
