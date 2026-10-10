"""Local document, visual, audio and code segments with navigable locations."""
from dataclasses import dataclass, field
from pathlib import Path
from array import array
import math
import csv
import ast
import io
import json
import os
import re
import shutil
import subprocess
import process_lifecycle
import tempfile
import threading
import wave
from PIL import Image, ImageOps
import pymupdf
import imageio_ffmpeg

IMAGE = {'.jpg','.jpeg','.png','.webp','.bmp','.tif','.tiff','.gif'}
AUDIO = {'.mp3','.wav','.flac','.m4a','.aac','.ogg','.opus','.wma'}
VIDEO = {'.mp4','.mkv','.mov','.avi','.webm','.m4v','.wmv','.mpeg','.mpg'}
CODE = {'.py','.js','.jsx','.ts','.tsx','.java','.kt','.go','.rs','.c','.h','.cpp','.hpp','.cs','.swift','.rb','.php','.sh','.ps1','.sql','.ipynb'}
EXCLUDE = {'node_modules','vendor','dist','build','target','.git','.venv','venv','__pycache__','bin','obj'}

@dataclass
class Segment:
    kind: str = 'text'
    text: str = ''
    page: int = 0
    seconds: float = -1
    end: float = -1
    line: int = 0
    end_line: int = 0
    symbol: str = ''
    boxes: list = field(default_factory=list)
    media: bytes = b''
    def location(self):
        if self.page:
            return f'Page {self.page}'
        if self.seconds >= 0:
            return f'{int(self.seconds)//60}:{int(self.seconds)%60:02d}'
        if self.line:
            return f'{self.symbol + " · " if self.symbol else ""}Lines {self.line}–{self.end_line}'
        return {'image':'Photo / screenshot','filename':'Filename only'}.get(self.kind,'Document text')
    def metadata(self):
        return json.dumps({k:v for k,v in vars(self).items() if k not in ('media','text','kind')},ensure_ascii=False)

def chunks(text, size=2400, overlap=240):
    text=text.strip()
    for offset in range(0,len(text),size-overlap):
        yield text[offset:offset+size]
        if offset+size >= len(text):
            break

def ocr_program(settings):
    configured=settings.get('ocr_path','').strip()
    if configured:
        return configured if Path(configured).is_file() else None
    found=shutil.which('tesseract')
    if found:
        return found
    for root in (os.environ.get('LOCALAPPDATA',''),os.environ.get('ProgramFiles','C:/Program Files')):
        for folder in ('Programs/Tesseract-OCR','Tesseract-OCR'):
            candidate=Path(root)/folder/'tesseract.exe'
            if candidate.is_file():
                return str(candidate)
    return None

def ocr(image, settings):
    program=ocr_program(settings)
    if not program:
        return '',[]
    buffer=io.BytesIO()
    image.save(buffer,format='PNG')
    args=[program,'stdin','stdout','-l',settings.get('ocr_languages','eng') or 'eng','-c','tessedit_create_tsv=1']
    if settings.get('ocr_data'):
        args+=['--tessdata-dir',str(Path(settings['ocr_data']))]
    # Use literal subprocess arguments; wrappers can leave quotes inside Windows
    # data paths containing spaces. PNG stdin also avoids temporary image files.
    result=process_lifecycle.run(args,input=buffer.getvalue(),capture_output=True,timeout=90,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
    if result.returncode:
        raise RuntimeError('OCR failed: '+result.stderr.decode('utf-8',errors='replace')[-1000:])
    words=[]
    boxes=[]
    rows=csv.DictReader(io.StringIO(result.stdout.decode('utf-8',errors='replace')),delimiter='\t',quoting=csv.QUOTE_NONE)
    for row in rows:
        word=row.get('text','') or ''
        if word.strip() and float(row['conf'])>=0:
            words.append(word)
            boxes.append([word,int(row['left']),int(row['top']),int(row['width']),int(row['height'])])
    return ' '.join(words),boxes

def image_bytes(image):
    image=ImageOps.exif_transpose(image).convert('RGB')
    image.thumbnail((1024,1024))
    buffer=io.BytesIO()
    image.save(buffer,format='JPEG',quality=90)
    return buffer.getvalue()

def ffmpeg(settings):
    configured=settings.get('ffmpeg_path','').strip()
    return configured or shutil.which('ffmpeg') or imageio_ffmpeg.get_ffmpeg_exe()

def process(args):
    return process_lifecycle.popen(args,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))

def audio_segments(path,settings,checkpoint):
    """Decode one bounded PCM stream; overlapping windows retain speech context."""
    proc=process([ffmpeg(settings),'-nostdin','-v','error','-i',str(path),'-vn','-ac','1','-ar','16000','-f','s16le','pipe:1'])
    errors=[]
    reader=threading.Thread(target=lambda:errors.append(proc.stderr.read()),daemon=True)
    reader.start()
    window=30*16000*2
    step=25*16000*2
    buffer=b''
    start=0.0
    try:
        while True:
            checkpoint()
            chunk=proc.stdout.read(min(65536,window-len(buffer)))
            if chunk:
                buffer+=chunk
            if len(buffer)==window or (not chunk and buffer):
                output=io.BytesIO()
                with wave.open(output,'wb') as wav:
                    wav.setnchannels(1); wav.setsampwidth(2); wav.setframerate(16000); wav.writeframes(buffer)
                samples=array('h',buffer[:len(buffer)//2*2])
                rms=math.sqrt(sum(sample*sample for sample in samples)/max(1,len(samples)))
                if rms>=20:
                    yield Segment(kind='audio',seconds=start,end=start+len(buffer)/32000,media=output.getvalue())
                if not chunk:
                    break
                buffer=buffer[step:]
                start+=25
            elif not chunk:
                break
        proc.wait(timeout=15)
        reader.join(timeout=2)
        if proc.returncode:
            raise RuntimeError('Audio decode failed: '+b''.join(errors).decode(errors='replace')[-1000:])
    finally:
        if proc.poll() is None:
            proc.kill()
        proc.stdout.close(); proc.stderr.close(); proc.wait()

def video_segments(path,settings,checkpoint):
    """Frames every three seconds and on scene changes, retaining original PTS."""
    import queue
    stamps=queue.Queue()
    errors=[]
    interval=max(1,int(settings.get('video_interval',3)))
    filters=f"select='isnan(prev_selected_t)+gte(t-prev_selected_t,{interval})+gt(scene,0.35)',scale='min(1024,iw)':-2,showinfo"
    proc=process([ffmpeg(settings),'-nostdin','-v','info','-i',str(path),'-an','-vf',filters,'-fps_mode','vfr','-f','image2pipe','-vcodec','mjpeg','pipe:1'])
    def stderr():
        for raw in iter(proc.stderr.readline,b''):
            line=raw.decode(errors='replace')
            if 'showinfo' in line and (match:=re.search(r'pts_time:([\d.eE+-]+)',line)):
                stamps.put(float(match[1]))
            errors.append(line)
            if len(errors)>40:
                errors.pop(0)
    reader=threading.Thread(target=stderr,daemon=True)
    reader.start()
    buffer=b''
    try:
        while True:
            checkpoint()
            chunk=proc.stdout.read(65536)
            buffer+=chunk
            while True:
                start=buffer.find(b'\xff\xd8')
                end=buffer.find(b'\xff\xd9',max(0,start+2))
                if start<0 or end<0:
                    break
                frame=buffer[start:end+2]
                buffer=buffer[end+2:]
                checkpoint()
                try:
                    seconds=stamps.get(timeout=10)
                except queue.Empty:
                    raise RuntimeError('Video frame timestamp unavailable; refusing an inaccurate location')
                yield Segment(kind='video',seconds=max(0,seconds),end=max(0,seconds)+interval,media=frame)
            if not chunk:
                break
            if len(buffer)>32*1024*1024:
                raise RuntimeError('Video decoder frame exceeds the supported buffer')
        proc.wait(timeout=15)
        reader.join(timeout=2)
        if proc.returncode:
            raise RuntimeError('Video decode failed: '+''.join(errors)[-1000:])
    finally:
        if proc.poll() is None:
            proc.kill()
        proc.stdout.close(); proc.stderr.close(); proc.wait()

def extract(path,settings,read_text,checkpoint):
    suffix=path.suffix.lower()
    if suffix in CODE:
        # Code content is never included unless its separate search is enabled.
        yield Segment(kind='filename')
    elif suffix in IMAGE and settings.get('index_images',True):
        with Image.open(path) as original:
            image=ImageOps.exif_transpose(original).convert('RGB')
            text,boxes=ocr(image,settings) if settings.get('ocr_enabled',True) else ('',[])
            yield Segment(kind='image',text=text,boxes=boxes,media=image_bytes(image) if settings['semantic'] else b'')
    elif suffix=='.pdf':
        with pymupdf.open(path) as doc:
            if doc.needs_pass:
                raise ValueError('Password-protected PDF')
            for number,page in enumerate(doc):
                checkpoint()
                text=page.get_text(sort=True)
                if not text.strip():
                    scale=min(2,2048/max(page.rect.width,page.rect.height,1))
                    pixmap=page.get_pixmap(matrix=pymupdf.Matrix(scale,scale),alpha=False)
                    image=Image.frombytes('RGB',[pixmap.width,pixmap.height],pixmap.samples)
                    text,boxes=ocr(image,settings) if settings.get('ocr_enabled',True) else ('',[])
                    # Store OCR rectangles in PDF points for highlights.
                    boxes=[[word,x/scale,y/scale,w/scale,h/scale] for word,x,y,w,h in boxes]
                    yield Segment(kind='pdf',text=text,page=number+1,boxes=boxes,media=image_bytes(image) if settings['semantic'] else b'')
                else:
                    for passage in chunks(text):
                        yield Segment(kind='pdf',text=passage,page=number+1)
    elif suffix in AUDIO and settings.get('index_audio',True) and settings['semantic']:
        yield from audio_segments(path,settings,checkpoint)
    elif suffix in VIDEO and settings.get('index_video',True) and settings['semantic']:
        yield from video_segments(path,settings,checkpoint)
        if settings.get('index_audio',True):
            # Silent videos legitimately have no audio stream.
            probe=process([ffmpeg(settings),'-nostdin','-i',str(path)])
            _,info=probe.communicate(timeout=30)
            if b'Audio:' in info:
                for segment in audio_segments(path,settings,checkpoint):
                    segment.kind='video_audio'
                    yield segment
    else:
        text=read_text(path)
        for passage in chunks(text):
            yield Segment(text=passage)
        if not text:
            yield Segment(kind='filename')

def project_files(root):
    root=Path(root).resolve()
    try:
        top=process_lifecycle.run(['git','-C',str(root),'rev-parse','--show-toplevel'],capture_output=True,timeout=10,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
        if top.returncode==0:
            repo=Path(top.stdout.decode().strip()).resolve()
            result=process_lifecycle.run(['git','-C',str(repo),'ls-files','--cached','--others','--exclude-standard','-z'],capture_output=True,timeout=30,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
            if result.returncode:
                raise RuntimeError('Cannot read project files with Git')
            for name in result.stdout.decode('utf-8',errors='replace').split('\0'):
                if name:
                    path=(repo/name).resolve()
                    if path.is_relative_to(root) and path.is_file() and not path.is_symlink():
                        yield path
            return
    except FileNotFoundError:
        pass
    for base,dirs,names in os.walk(root,followlinks=False):
        dirs[:]=[d for d in dirs if d.lower() not in EXCLUDE and not d.startswith('.') and not (Path(base)/d).is_symlink() and not (Path(base)/d).is_junction()]
        for name in names:
            yield Path(base)/name

def code_segments(path):
    if path.suffix.lower() not in CODE or path.stat().st_size>2*1024*1024:
        return
    text=path.read_text(encoding='utf-8',errors='replace')
    if path.suffix.lower()=='.ipynb':
        notebook=json.loads(text)
        for index,cell in enumerate(notebook.get('cells',[])):
            if cell.get('cell_type')=='code':
                source=''.join(cell.get('source',[]))
                for passage in chunks(source):
                    yield Segment(kind='code',text=passage,symbol=f'Notebook cell {index+1}')
        return
    lines=text.splitlines()
    if path.suffix.lower()=='.py':
        try:
            tree=ast.parse(text)
            nodes=[node for node in ast.walk(tree) if isinstance(node,(ast.FunctionDef,ast.AsyncFunctionDef))]
            if nodes:
                for node in nodes:
                    start=min([node.lineno]+[d.lineno for d in node.decorator_list])
                    for passage in chunks('\n'.join(lines[start-1:node.end_lineno])):
                        yield Segment(kind='code',text=passage,line=start,end_line=node.end_lineno,symbol=node.name)
                return
        except SyntaxError:
            pass
    if path.suffix.lower() in {'.js','.jsx','.ts','.tsx','.java','.kt','.go','.rs','.c','.h','.cpp','.hpp','.cs','.swift','.php'}:
        # Mask comments and strings without moving source offsets. Balanced braces
        # locate named functions/methods rather than returning the entire file.
        masked=re.sub(r'//[^\n]*|/\*[\s\S]*?\*/|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|`(?:\\.|[^`\\])*`',lambda m:re.sub(r'[^\n]',' ',m.group()),text)
        pattern=r'(?:\b(?:function|fn|func|fun)\s+([\w]+)\s*\([^)]*\)[^{;]*|\b(?:const|let|var)\s+([\w]+)\s*=\s*(?:async\s*)?\([^)]*\)\s*=>\s*|\b([\w]+)\s*\([^;{}]*\)\s*(?:async\s*)?(?:throws\s+[\w., ]+)?\s*)\{'
        found=False
        for match in re.finditer(pattern,masked):
            symbol=next(name for name in match.groups() if name)
            if symbol in {'if','for','while','switch','catch','with','synchronized'}:
                continue
            start=match.start()
            opening=match.end()-1
            depth=1
            end=opening+1
            while end<len(masked) and depth:
                depth+=(masked[end]=='{')-(masked[end]=='}')
                end+=1
            if depth:
                continue
            first=text.count('\n',0,start)+1
            last=text.count('\n',0,end)+1
            for passage in chunks('\n'.join(lines[first-1:last])):
                yield Segment(kind='code',text=passage,line=first,end_line=last,symbol=symbol)
            found=True
        if found:
            return
    # Keep source positions even in languages without a local parser.
    for start in range(0,len(lines),50):
        end=min(len(lines),start+60)
        body='\n'.join(lines[start:end])
        match=re.search(r'(?:function\s+|fn\s+|func\s+|def\s+)([\w]+)',body)
        for passage in chunks(body):
            yield Segment(kind='code',text=passage,line=start+1,end_line=end,symbol=match[1] if match else '')
