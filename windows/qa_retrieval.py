"""Reproducible generated-file evaluation; does not claim real-world semantic accuracy."""
import json
import math
from pathlib import Path
import statistics
import struct
import subprocess
import time
import wave
import zipfile
from PIL import Image,ImageDraw,ImageFont
import pymupdf
from content import audio_segments,video_segments
from engine import Engine,DEFAULTS
from office_content import office_segments
from security import MAGIC
import imageio_ffmpeg

home=Path(__file__).parent/'.smoke/evaluation-v02';home.mkdir(parents=True,exist_ok=True)
source=home/'source';source.mkdir(exist_ok=True)
for i in range(1500):
    (source/f'note-{i:04d}.txt').write_text(f'Document {i}: Invoice INV-{i:06d} describes office supplies and receipt number REF-{i:06d}.')
font=ImageFont.truetype('C:/Windows/Fonts/arial.ttf',36)
for i in range(14):
    image=Image.new('RGB',(1100,300),'white');draw=ImageDraw.Draw(image)
    draw.text((30,40),'Payment declined error',font=font,fill='black')
    draw.text((30,130),f'Invoice SCREEN-{i:04d}',font=font,fill='black');image.save(source/f'screen-{i:02d}.png')
for i in range(30):
    path=source/f'lease-{i:02d}.pdf'
    if path.exists():path.unlink()
    with pymupdf.open() as doc:
        for page in range(463 if i==0 else 3):
            p=doc.new_page();p.insert_text((40,60),f'Lease {i}, page {page+1}. Routine document text and rental payment terms.')
            if page==(462 if i==0 else 2):p.insert_text((40,100),f'Pets require written permission. CLAUSE-{i:04d}')
        doc.save(path)
project=source/'project';project.mkdir(exist_ok=True)
subprocess.run(['git','init','-q',str(project)],check=True)
(project/'functions.py').write_text('\n'.join(f'def action_{i:04d}():\n    """Handle code lookup CODE-{i:04d}."""\n    return {i}\n' for i in range(68)))
engine=Engine(home);settings=dict(DEFAULTS,roots=[str(source)],ocr_enabled=True,code_enabled=True,code_roots=[str(project)])
start=time.perf_counter();summary=engine.index(settings);index_seconds=time.perf_counter()-start
queries=[(f'INV-{i:06d}',f'note-{i:04d}.txt') for i in range(90)]
queries += [(f'SCREEN-{i:04d}',f'screen-{i:02d}.png') for i in range(14)]
queries += [(f'CLAUSE-{i:04d}',f'lease-{i:02d}.pdf') for i in range(7)]
queries += [(f'code: CODE-{i:04d}','functions.py') for i in range(68)]
passed=0;times=[];failures=[]
for query,name in queries:
    start=time.perf_counter();results=engine.search(query,settings);times.append((time.perf_counter()-start)*1000)
    if results and Path(results[0].path).name==name:passed+=1
    else:failures.append(query)
last_page=engine.search('CLAUSE-0000',settings)[0].page
# One hour of generated tone validates complete overlapping audio segmentation, not speech accuracy.
audio=home/'hour.wav'
with wave.open(str(audio),'wb') as out:
    out.setnchannels(1);out.setsampwidth(2);out.setframerate(16000)
    tone=b''.join(struct.pack('<h',int(1500*math.sin(2*math.pi*440*i/16000))) for i in range(16000))
    for _ in range(3600):out.writeframesraw(tone)
segments=list(audio_segments(audio,dict(settings,ffmpeg_path=imageio_ffmpeg.get_ffmpeg_exe()),lambda:None))
assert segments[0].seconds==0 and segments[-1].end>=3600
video=home/'minute.mp4'
subprocess.run([imageio_ffmpeg.get_ffmpeg_exe(),'-y','-f','lavfi','-i','color=c=purple:s=320x180:r=3:d=60','-an','-c:v','libx264','-pix_fmt','yuv420p',str(video)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True,creationflags=0x08000000)
frames=list(video_segments(video,dict(settings,ffmpeg_path=imageio_ffmpeg.get_ffmpeg_exe()),lambda:None))
assert frames and frames[-1].seconds>=57
report={'fixture_files':engine.indexed_file_count(),'index_summary':summary,'index_seconds':round(index_seconds,2),'retrieval':'exact keyword/OCR; no model used','queries':len(queries),'top1_passed':passed,'failures':failures,'warm_search_median_ms':round(statistics.median(times[1:]),2),'longest_pdf_pages':463,'matched_last_page':last_page,'one_hour_audio_segments':len(segments),'last_audio_end_seconds':segments[-1].end,'one_minute_video_frames':len(frames),'last_video_frame_seconds':frames[-1].seconds,'index_protected':engine.db.read_bytes().startswith(MAGIC),'limitations':'Generated documents and screenshots, tone audio and flat video. Validates extraction, exact retrieval, indexing size and timestamps; not natural-media semantic accuracy.'}
(home/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
assert passed==len(queries) and last_page==463
