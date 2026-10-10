"""Optional live-model evaluation against a running multimodal llama-server."""
import json
from pathlib import Path
import subprocess
import wave
import urllib.request
from engine import Engine,DEFAULTS

home=Path(__file__).parent/'.smoke/live-model-v02';home.mkdir(parents=True,exist_ok=True)
source=home/'source';source.mkdir(exist_ok=True)
notes={'english.txt':'A golden retriever plays beside crashing ocean waves. The dog is running along the sandy beach.','bengali.txt':'আমার কুকুর সমুদ্র সৈকতে খেলছে।','arabic.txt':'كلبي يلعب على شاطئ البحر.','invoice.txt':'Invoice INV-66384 has an overdue balance.','garden.txt':'Tomatoes need sunlight and regular watering.','network.txt':'The request failed with a timeout. Retry the request with exponential backoff.','sleep.txt':'Sleep supports memory consolidation and learning. Consistent bedtimes help people feel rested.'}
for name,text in notes.items():(source/name).write_text(text,encoding='utf-8')
script=home/'speak.ps1'
script.write_text("$voice=New-Object -ComObject SAPI.SpVoice\n$stream=New-Object -ComObject SAPI.SpFileStream\n$stream.Open('"+str((home/'speech.wav').resolve())+"',3,$false)\n$voice.AudioOutputStream=$stream\n[void]$voice.Speak('Sleep improves memory and supports learning. People who sleep too little often feel tired. Doctors recommend consistent bedtimes.')\n$stream.Close()\n",encoding='utf-8')
subprocess.run(['powershell','-NoProfile','-File',str(script)],check=True,stdout=subprocess.DEVNULL,creationflags=0x08000000)
with wave.open(str(home/'speech.wav'),'rb') as incoming:
    params=incoming.getparams();speech=incoming.readframes(incoming.getnframes())
with wave.open(str(source/'podcast.wav'),'wb') as out:
    out.setparams(params)
    second=b'\0'*(params.framerate*params.nchannels*params.sampwidth)
    for _ in range(60):out.writeframesraw(second)
    out.writeframesraw(speech)
    for _ in range(20):out.writeframesraw(second)
# Optional regression using the user's previously identified dog photo, kept only in ignored fixtures.
import shutil
photo=Path('H:/DOWNLOADS/20260718_200308.jpg')
if photo.exists():shutil.copy2(photo,source/'visual-example.jpg')
from PIL import Image,ImageDraw
screenshot=Image.new('RGB',(900,500),'white')
ImageDraw.Draw(screenshot).text((40,40),'Image upload fallback. Images are pasted inline. Browser compatibility.',fill='black')
screenshot.save(source/'compatibility.png')
settings=dict(DEFAULTS,roots=[str(source)],semantic=True,server_mode='Remote',remote_url='http://127.0.0.1:8787',remote_model_id='embeddinggemma-2',ocr_enabled=False)
engine=Engine(home);summary=engine.index(settings)
checks=[]
for query,expected in [('dog on the beach',{'english.txt','bengali.txt','arabic.txt'}),('সমুদ্র সৈকতে কুকুর',{'english.txt'}),('كلب على الشاطئ',{'english.txt'}),('INV-66384',{'invoice.txt'}),('where they talk about sleep',{'podcast.wav'}),('images of dogs',{'visual-example.jpg'})]:
    results=engine.search(query,settings)
    top=[{'file':Path(item.path).name,'score':round(item.score,3),'seconds':item.seconds,'reason':item.reasons} for item in results[:5]]
    found={item['file'] for item in top}
    passed=expected.issubset(found)
    if 'visual-example.jpg' in expected:passed=bool(top and top[0]['file']=='visual-example.jpg')
    if 'podcast.wav' in expected:passed=any(item['file']=='podcast.wav' and 35<=item['seconds']<=75 for item in top)
    checks.append({'query':query,'passed':passed,'expected_top5':sorted(expected),'results':top})
report={'index_summary':summary,'model':'EmbeddingGemma 2 BF16 (existing local llama-server)','fixture':'7 short multilingual notes, one generated TTS podcast with speech after 60 seconds, one known dog photo and one unrelated generated screenshot','checks':checks,'passed':sum(item['passed'] for item in checks),'total':len(checks),'limitations':'Small generated test set, not a real-world accuracy benchmark. Includes one photo regression; does not establish broad photo or natural-video accuracy.'}
(home/'report.json').write_text(json.dumps(report,indent=2,ensure_ascii=False),encoding='utf-8')
print(json.dumps(report,indent=2,ensure_ascii=True))
