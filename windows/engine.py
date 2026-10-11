"""Persistent settings, incremental index, and search shared by the desktop UI."""
from contextlib import contextmanager
from dataclasses import dataclass
import numpy as np
from collections import OrderedDict
import re
import shutil
import logging
import json
import os
from pathlib import Path
import sqlite3
import threading
import time
import urllib.request
import zipfile
import secrets
from parser_worker import isolated_extract
from security import MAGIC,seal,unseal,protect_secret,reveal_secret,contains_credentials,excluded_path
import xml.etree.ElementTree as ET
from vectordash_core import safe, read_text, embed, cosine, embedding_request, server_url
from content import Segment, extract, project_files, code_segments, ocr_program, IMAGE, AUDIO, VIDEO

DEFAULTS = {
    'server_mode': 'Local', 'remote_url': '', 'remote_key': '', 'remote_model_id': 'embeddinggemma-2',
    'projector': '', 'index_images': True, 'index_audio': True, 'index_video': True,
    'ocr_enabled': True, 'ocr_path': '', 'ocr_data': '', 'ocr_languages': 'eng', 'ffmpeg_path': '',
    'video_interval': 3, 'code_enabled': False, 'code_roots': [],
    'roots': [], 'mode': 'CPU / system RAM', 'server': '', 'model': '', 'device': '',
    'semantic': False, 'theme': 'System', 'hotkey': 'Ctrl+Shift+Space',
    'startup': False, 'start_hidden': True, 'close_behavior': 'Hide to tray',
    'minimize_behavior': 'Taskbar', 'auto_model': True,
    'schedule_mode': 'Manual', 'schedule_time': '02:00', 'schedule_days': [0,1,2,3,4,5,6],
    'idle_minutes': 5, 'cpu_limit': 25, 'gpu_limit': 20, 'check_cpu': True,
    'check_gpu': False, 'check_user_idle': True, 'ac_only': True, 'idle_interval': 60, 'last_success': '',
    'mcp_lan': False, 'mcp_enabled': False, 'mcp_port': 8790, 'mcp_auth': True, 'mcp_token': '',
    'exclude_sensitive': True, 'excluded_folders': [], 'index_excluded_types': '', 'search_excluded_types': '', 'search_excluded_files': [],
    'last_schedule_slot': '', 'schedule_history': [], 'max_run_minutes': 120
}

def data_home():
    override = os.environ.get('VECTORDASH_DATA_HOME') or os.environ.get('DIGUP_DATA_HOME')
    base = Path(os.environ.get('LOCALAPPDATA') or str(Path.home()))
    if override:
        return Path(override)
    target=base/'VectorDash'
    legacy=base/'DigUp'
    if not target.exists() and (legacy/'settings.json').exists():
        shutil.move(str(legacy),str(target))
        settings_file=target/'settings.json'
        values=json.loads(settings_file.read_text(encoding='utf-8-sig'))
        for key in ('server','model','projector','ocr_path','ocr_data','ffmpeg_path'):
            value=values.get(key,'')
            if value and Path(value).is_relative_to(legacy):
                values[key]=str(target/Path(value).relative_to(legacy))
        settings_file.write_text(json.dumps(values,indent=2),encoding='utf-8')
    return target

class Settings:
    def __init__(self, home=None):
        self.home = Path(home) if home else data_home()
        self.home.mkdir(parents=True, exist_ok=True)
        self.path = self.home/'settings.json'
        self.values = dict(DEFAULTS)
        if self.path.exists():
            self.values.update(json.loads(self.path.read_text(encoding='utf-8-sig')))
        for key in ('remote_key','mcp_token'):
            self.values[key]=reveal_secret(self.values.get(key,''))
        if not self.values['mcp_token']: self.values['mcp_token']=secrets.token_urlsafe(32)
    def save(self):
        temp = self.path.with_suffix('.tmp')
        stored=dict(self.values)
        for key in ('remote_key','mcp_token'): stored[key]=protect_secret(stored.get(key,''))
        temp.write_text(json.dumps(stored, indent=2), encoding='utf-8')
        temp.replace(self.path)


def model_signature(settings):
    if not settings['semantic']:
        return ''
    if settings.get('server_mode')=='Remote':
        return json.dumps(['remote',server_url(settings),settings.get('remote_model_id','embeddinggemma-2'),4])
    path = Path(settings['model'])
    stat = path.stat()
    # Include the extraction/embedding algorithm version to invalidate old vectors.
    projection = Path(settings['projector']) if settings.get('projector') else None
    encoder = [str(projection.resolve()), projection.stat().st_size, projection.stat().st_mtime_ns] if projection else []
    return json.dumps([str(path.resolve()), stat.st_size, stat.st_mtime_ns, encoder, 4])


def verify_server_model(settings):
    if not settings['semantic']:
        return
    with urllib.request.urlopen(urllib.request.Request(server_url(settings)+'/props',headers={'Authorization':'Bearer '+settings['remote_key']} if settings.get('remote_key') else {}),timeout=5) as response:
        props=json.load(response)
        running=props.get('model_path')
    if settings.get('server_mode')=='Remote':
        expected=settings.get('remote_model_id','embeddinggemma-2').casefold()
        if expected and expected not in str(running or props.get('model_alias','')).casefold():
            raise ValueError('Remote server model does not match the configured model identifier.')
        return
    if not running or os.path.normcase(str(Path(running).resolve())) != os.path.normcase(str(Path(settings['model']).resolve())):
        raise ValueError('The running server uses a different model. Stop it, then start the selected model before indexing/searching.')


@dataclass
class SearchResult:
    score: float
    path: str
    snippet: str
    kind: str = 'text'
    page: int = 0
    seconds: float = -1
    end: float = -1
    line: int = 0
    end_line: int = 0
    symbol: str = ''
    boxes: list = None
    embedding_source: str = ''
    query: str = ''
    semantic_score: float = 0
    keyword_score: float = 0
    reasons: list = None
    def __iter__(self):
        return iter((self.score,self.path,self.snippet))
    def location(self):
        return Segment(kind=self.kind,page=self.page,seconds=self.seconds,line=self.line,end_line=self.end_line,symbol=self.symbol).location()

class Engine:
    def __init__(self, home, policy=None):
        self.db = Path(home)/'index.sqlite'
        self.protected=os.name=='nt'
        self.storage_lock=threading.RLock()
        self.storage_version=0
        self.snapshot=None
        if self.protected and self.db.exists():
            stored=self.db.read_bytes()
            if stored.startswith(MAGIC): self.snapshot=unseal(stored)
            else:
                with sqlite3.connect(self.db) as legacy:
                    legacy.execute('PRAGMA wal_checkpoint(TRUNCATE)')
                    self.snapshot=legacy.serialize()
                    self.snapshot=self.snapshot[:18]+b'\x01\x01'+self.snapshot[20:]
                legacy.close()
                self.persist(self.snapshot)
                for suffix in ('-wal','-shm'):
                    Path(str(self.db)+suffix).unlink(missing_ok=True)
        self.cancel = threading.Event()
        self.paused = threading.Event()
        self.inference = threading.Lock()
        self.query_waiting = threading.Event()
        self.search_cache_lock=threading.RLock()
        self.search_cache={}
        self.query_vectors=OrderedDict()
        with self.connect() as db:
            db.execute('PRAGMA journal_mode=WAL')
            db.execute('CREATE TABLE IF NOT EXISTS files(path TEXT PRIMARY KEY, text TEXT, vector TEXT)')
            db.execute('CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY, value TEXT)')
            columns = {row[1] for row in db.execute('PRAGMA table_info(files)')}
            for name in ('mtime','size'):
                if name not in columns:
                    db.execute(f'ALTER TABLE files ADD COLUMN {name} INTEGER DEFAULT -1')
            db.execute('CREATE TABLE IF NOT EXISTS segments(path TEXT, namespace TEXT, kind TEXT, text TEXT, vector TEXT, location TEXT)')
            db.execute('CREATE INDEX IF NOT EXISTS segments_path ON segments(path,namespace)')
            db.execute("CREATE VIRTUAL TABLE IF NOT EXISTS words USING fts5(path,text,tokenize='unicode61')")
            # Preserve readable legacy results until a successful replacement.
            if not db.execute('SELECT 1 FROM segments LIMIT 1').fetchone():
                db.execute("INSERT INTO segments SELECT path,'files','text',text,vector,'{}' FROM files")
                db.execute('INSERT INTO words(rowid,path,text) SELECT rowid,path,text FROM segments')
        if not policy or policy.get('exclude_sensitive',True):self.remove_sensitive_legacy_content()

    def remove_sensitive_legacy_content(self):
        # Upgrade older indexes before exposing them to search or agents.
        with self.connect() as db:
            bad={path for path,text in db.execute('SELECT path,text FROM segments') if not safe(Path(Path(path).name)) or contains_credentials(text)}
            for path in bad:
                db.execute('DELETE FROM segments WHERE path=?',(path,))
                db.execute('DELETE FROM files WHERE path=?',(path,))
                db.execute('DELETE FROM words WHERE path=?',(path,))
        return len(bad)

    def indexed_file_count(self):
        with self.connect() as db:
            return db.execute('SELECT count(DISTINCT path) FROM segments').fetchone()[0]

    def cached_index(self,namespace):
        with self.search_cache_lock:
            # Include WAL changes as well as checkpointed database changes.
            version=self.storage_version if self.protected else tuple((path.stat().st_mtime_ns,path.stat().st_size) if path.exists() else None for path in (self.db,Path(str(self.db)+'-wal')))
            key=(version,namespace)
            if key in self.search_cache:
                return self.search_cache[key]
            with self.connect() as db:
                db.execute('BEGIN')
                metadata=dict(db.execute('SELECT key,value FROM metadata'))
                rows=list(db.execute('SELECT rowid,path,kind,text,vector,location FROM segments WHERE namespace=?',(namespace,)))
            records=[]
            groups={}
            for i,(rowid,path,kind,text,stored,location) in enumerate(rows):
                vector=json.loads(stored)
                if vector:
                    groups.setdefault(len(vector),[]).append((i,vector))
                records.append((rowid,path,kind,text,(path+'\n'+text).casefold(),location))
            matrices={}
            for dimension,entries in groups.items():
                matrix=np.asarray([vector for i,vector in entries],dtype=np.float32)
                norms=np.linalg.norm(matrix,axis=1,keepdims=True)
                matrix/=np.maximum(norms,1e-30)
                matrices[dimension]=([i for i,vector in entries],matrix)
            value=(records,matrices,metadata)
            self.search_cache={k:v for k,v in self.search_cache.items() if k[0]==version}
            self.search_cache[key]=value
            return value

    def clear_query_cache(self):
        with self.search_cache_lock:
            self.query_vectors.clear()

    def persist(self,snapshot):
        encrypted=seal(snapshot)
        temp=self.db.with_suffix('.protected.tmp')
        temp.write_bytes(encrypted)
        temp.replace(self.db)
        self.snapshot=snapshot
        self.storage_version+=1

    @contextmanager
    def connect(self):
        connection=sqlite3.connect(':memory:' if self.protected else self.db,timeout=30)
        if self.protected:
            with self.storage_lock:
                if self.snapshot: connection.deserialize(self.snapshot)
            connection.execute('PRAGMA temp_store=MEMORY')
        changes=connection.total_changes
        schema=connection.execute('PRAGMA schema_version').fetchone()[0]
        try:
            with connection:
                yield connection
            if self.protected and (connection.total_changes!=changes or connection.execute('PRAGMA schema_version').fetchone()[0]!=schema):
                with self.storage_lock: self.persist(connection.serialize())
        finally:
            connection.close()

    def checkpoint(self, condition=lambda: True, wait_query=True):
        while True:
            if self.cancel.is_set():
                raise InterruptedError('Indexing stopped; previous index retained')
            if condition() and not self.paused.is_set() and not (wait_query and self.query_waiting.is_set()):
                return
            self.cancel.wait(.25)

    def infer(self,segment,path,condition,settings=None):
        def request(text):
            with self.inference:
                return embedding_request('title: '+path.name+' | text: '+text,settings) if settings and settings.get('server_mode')=='Remote' else embedding_request('title: '+path.name+' | text: '+text)
        checkpoint=lambda:self.checkpoint(condition)
        if segment.media:
            checkpoint()
            with self.inference:
                # Media must be embedded alone; a filename would contaminate its meaning.
                visual=embedding_request(segment.media,settings) if settings and settings.get('server_mode')=='Remote' else embedding_request(segment.media)
            if segment.text:
                # Separate text and media vectors retain both retrieval spaces.
                textual=embed(segment.text,checkpoint=checkpoint,request=request)
                return [visual,textual]
            return [visual]
        return [embed(segment.text or path.name,checkpoint=checkpoint,request=request)]

    def index(self, settings, progress=lambda text: None, condition=lambda: True, force=False):
        signature=model_signature(settings)
        verify_server_model(settings)
        content_options={key:settings.get(key) for key in ('index_images','index_audio','index_video','ocr_enabled','ocr_path','ocr_data','ocr_languages','video_interval','code_enabled','code_roots','exclude_sensitive','excluded_folders','index_excluded_types')}
        detected_ocr=ocr_program(settings)
        content_options['detected_ocr']=detected_ocr
        if settings.get('ocr_enabled',True):
            candidates=[Path(detected_ocr)] if detected_ocr else []
            if settings.get('ocr_data'):
                candidates.extend(Path(settings['ocr_data'])/(code+'.traineddata') for code in settings.get('ocr_languages','eng').split('+'))
            content_options['ocr_files']=[[str(path),path.stat().st_size,path.stat().st_mtime_ns] for path in candidates if path.is_file()]
        content_options['ffmpeg_path']=settings.get('ffmpeg_path','')
        configuration=json.dumps(content_options,sort_keys=True)
        warnings=[]
        if settings.get('ocr_enabled',True) and not ocr_program(settings):
            warnings.append('OCR unavailable: install Tesseract or set its path; screenshot exact text is not indexed')
        count=reused=skipped=segment_count=0
        kinds={}
        seen=set()
        checked_media=False
        with self.connect() as db:
            metadata=dict(db.execute('SELECT key,value FROM metadata'))
            previous={row[0]:row[1:] for row in db.execute('SELECT path,text,vector,mtime,size FROM files')}
            compatible=metadata.get('security_version')=='1' and metadata.get('signature')==signature and metadata.get('content_options')==configuration and metadata.get('segments_version')=='4' and not force
            db.execute('CREATE TEMP TABLE replacement(path TEXT PRIMARY KEY,text TEXT,vector TEXT,mtime INTEGER,size INTEGER)')
            db.execute('CREATE TEMP TABLE new_segments(path TEXT,namespace TEXT,kind TEXT,text TEXT,vector TEXT,location TEXT)')
            def add(path,namespace):
                nonlocal count,reused,skipped,segment_count,checked_media
                self.checkpoint(condition)
                path=Path(path)
                key=(str(path),namespace)
                if key in seen:
                    return
                seen.add(key)
                try:
                    stat=path.stat()
                    roots=settings.get('code_roots',[]) if namespace=='code' else settings['roots']
                    relative=next((path.relative_to(Path(root)) for root in roots if path.is_relative_to(Path(root))),Path(path.name))
                    if not safe(relative) or excluded_path(path,settings) or path.is_symlink() or getattr(stat,'st_file_attributes',0)&(0x2|0x400|0x1000|0x40000|0x400000):
                        return
                    old=previous.get(str(path))
                    if compatible and old and old[2:]==(stat.st_mtime_ns,stat.st_size):
                        db.execute('INSERT OR REPLACE INTO replacement VALUES(?,?,?,?,?)',(str(path),old[0],old[1],stat.st_mtime_ns,stat.st_size))
                        db.execute('INSERT INTO new_segments SELECT * FROM segments WHERE path=? AND namespace=?',key)
                        reused+=1
                    else:
                        progress(f'Reading {path.name} · {segment_count} content segments')
                        if namespace=='code':
                            segments=isolated_extract(path,settings,lambda:self.checkpoint(condition),code=True)
                        else:
                            media_kind=path.suffix.lower() in IMAGE|AUDIO|VIDEO
                            if settings['semantic'] and media_kind and not checked_media:
                                with urllib.request.urlopen(urllib.request.Request(server_url(settings)+'/props',headers={'Authorization':'Bearer '+settings['remote_key']} if settings.get('remote_key') else {}),timeout=5) as response:
                                    modalities=json.load(response).get('modalities',{})
                                needs_vision=(settings.get('index_images',True) or settings.get('index_video',True))
                                needs_audio=settings.get('index_audio',True)
                                if (settings.get('server_mode')!='Remote' and not settings.get('projector')) or needs_vision and not modalities.get('vision') or needs_audio and not modalities.get('audio'):
                                    raise RuntimeError('Photos/audio/video require the matching vision + audio encoder (mmproj). Download it in Models & hardware, restart the model, then rebuild the index.')
                                checked_media=True
                            from vectordash_core import TEXT
                            segments=extract(path,settings,read_text,lambda:self.checkpoint(condition)) if path.suffix.lower() in TEXT else isolated_extract(path,settings,lambda:self.checkpoint(condition))
                        file_text=[]
                        file_segments=0
                        for segment in segments:
                            self.checkpoint(condition)
                            if settings.get('exclude_sensitive',True) and contains_credentials(segment.text):
                                raise ValueError('Content excluded: potential credentials detected')
                            vectors=self.infer(segment,path,condition,settings) if settings['semantic'] else [[]]
                            for vector_number,vector in enumerate(vectors):
                                location=json.loads(segment.metadata())
                                location['embedding_source']='media' if segment.media and vector_number==0 else 'text'
                                db.execute('INSERT INTO new_segments VALUES(?,?,?,?,?,?)',(str(path),namespace,segment.kind,segment.text,json.dumps(vector),json.dumps(location)))
                            segment_count+=1
                            file_segments+=1
                            kinds[segment.kind]=kinds.get(segment.kind,0)+1
                            if file_segments%10==0:
                                progress(f'{path.name} · {file_segments} segments · {segment.location()}')
                            file_text.append(segment.text[:100])
                        if not file_segments:
                            db.execute('INSERT INTO new_segments VALUES(?,?,?,?,?,?)',(str(path),namespace,'filename','','[]','{}'))
                        db.execute('INSERT OR REPLACE INTO replacement VALUES(?,?,?,?,?)',(str(path),' '.join(file_text[:10]),'[]',stat.st_mtime_ns,stat.st_size))
                    count+=1
                except Exception as error:
                    if isinstance(error,(InterruptedError,sqlite3.Error)):
                        raise
                    db.execute('DELETE FROM new_segments WHERE path=? AND namespace=?',key)
                    skipped+=1
                    warnings.append(f'{path}: {error}')
                    logging.getLogger('vectordash').warning('Skipped %s: %s',path,error)
                    logging.getLogger('vectordash').debug('File failure details: %s',path,exc_info=True)
                    progress(f'Skipped {path.name}: {error}')
                    db.execute('INSERT INTO new_segments VALUES(?,?,?,?,?,?)',(str(path),namespace,'filename','','[]','{}'))
            for source in settings['roots']:
                if not Path(source).is_dir() or Path(source).is_symlink() or Path(source).is_junction():
                    raise ValueError(f'Source unavailable: {source}; previous index retained')
                errors=[]
                for base,dirs,names in os.walk(source,followlinks=False,onerror=errors.append):
                    self.checkpoint(condition)
                    allowed=[]
                    for directory in dirs:
                        path=Path(base)/directory
                        try:
                            attrs=getattr(path.stat(),'st_file_attributes',0)
                            if safe(path.relative_to(Path(source))) and not excluded_path(path,settings) and not path.is_symlink() and not path.is_junction() and not attrs&(0x2|0x400|0x1000|0x40000|0x400000):
                                allowed.append(directory)
                        except OSError:
                            skipped+=1
                    dirs[:]=allowed
                    for name in names:
                        add(Path(base)/name,'files')
                if errors:
                    for error in errors:
                        skipped+=1
                        logging.getLogger('vectordash').warning('Skipped inaccessible folder: %s',error)
            if settings.get('code_enabled'):
                for root in settings.get('code_roots',[]):
                    if not Path(root).is_dir() or Path(root).is_symlink() or Path(root).is_junction():
                        raise ValueError(f'Project unavailable: {root}; previous index retained')
                    for path in project_files(root):
                        from content import CODE
                        if path.suffix.lower() in CODE:
                            add(path,'code')
            self.checkpoint(condition)
            db.execute('DELETE FROM files')
            db.execute('INSERT INTO files SELECT * FROM replacement')
            db.execute('DELETE FROM segments')
            db.execute('INSERT INTO segments SELECT * FROM new_segments')
            db.execute('DELETE FROM words')
            db.execute('INSERT INTO words(rowid,path,text) SELECT rowid,path,text FROM segments')
            for key,value in [('signature',signature),('model',settings['model'] if settings['semantic'] else ''),('segments_version','4'),('security_version','1'),('content_options',configuration),('warnings',json.dumps(warnings,ensure_ascii=False))]:
                db.execute('INSERT OR REPLACE INTO metadata VALUES(?,?)',(key,value))
            total=db.execute('SELECT count(*) FROM segments').fetchone()[0]
            summary=', '.join(f'{n} {kind}' for kind,n in db.execute('SELECT kind,count(*) FROM segments GROUP BY kind'))
        result=f'{count} files indexed · {reused} reused · {skipped} skipped · {total} segments ({summary})'
        if warnings:
            result+=' · '+warnings[0]+(f' (+{len(warnings)-1} warnings)' if len(warnings)>1 else '')
        return result

    def search(self, query, settings):
        started=time.perf_counter()
        query=query.strip()
        if not query:
            return []
        explicit_code=query.lower().startswith('code:')
        code=explicit_code or settings.get('search_category')=='Code'
        if code:
            if not settings.get('code_enabled'):
                raise ValueError('Enable code search and choose project folders in Sources first.')
            query=query[5:].strip() if explicit_code else query
            if not query:
                return []
        excluded_files={os.path.normcase(os.path.abspath(path)) for path in settings.get('search_excluded_files',[])}
        records,matrices,metadata=self.cached_index('code' if code else 'files')
        vector=[]
        if settings['semantic']:
            verify_server_model(settings)
            signature=metadata.get('signature')
            if signature!=model_signature(settings):
                raise ValueError('Rebuild the index with the selected model and encoder before semantic search.')
            subject=re.sub(r'^(?:images?|photos?|pictures?)\s+(?:of\s+)?','',query,flags=re.I).strip() or query
            prompt=('task: code retrieval | query: ' if code else 'task: search result | query: ')+subject
            cache_key=(signature,server_url(settings),prompt)
            vector=self.query_vectors.get(cache_key)
            if vector is None:
                self.query_waiting.set()
                try:
                    with self.inference:
                        vector=embed(prompt,request=lambda text:embedding_request(text,settings)) if settings.get('server_mode')=='Remote' else embed(prompt)
                        self.query_vectors[cache_key]=vector
                        if len(self.query_vectors)>64:
                            self.query_vectors.popitem(last=False)
                finally:
                    self.query_waiting.clear()
        tokens=re.findall(r'[\w]+(?:[-./:][\w]+)*',query.casefold())
        stop={'a','an','the','in','on','at','of','to','and','or','where','they','about','find','shows','show','me','with','is','it'}
        tokens=[token for token in tokens if token not in stop] or tokens
        format_words={'image','images','photo','photos','picture','pictures','video','videos','clip','clips','audio','podcast','podcasts','document','documents','file','files'}
        subjects=[token for token in tokens if token not in format_words]
        if subjects:
            tokens=subjects
        if not tokens:
            return []
        scores=np.zeros(len(records),dtype=np.float32)
        if vector and matrices and len(vector) not in matrices:
            raise ValueError('Embedding dimensions do not match the index; rebuild with the selected model.')
        if vector and len(vector) in matrices:
            indices,matrix=matrices[len(vector)]
            query_vector=np.asarray(vector,dtype=np.float32)
            query_vector/=max(float(np.linalg.norm(query_vector)),1e-30)
            scores[indices]=matrix@query_vector
        modality_means={}
        if vector:
            groups={}
            for i,record in enumerate(records):
                channel=json.loads(record[5]).get('embedding_source','text')
                group=record[2] if channel=='media' else 'text'
                values=groups.setdefault(group,{})
                values[record[1]]=max(values.get(record[1],-1),float(scores[i]))
            modality_means={group:float(np.mean(list(values.values()))) for group,values in groups.items()}
        excluded={value.lower() if value.startswith('.') else '.'+value.lower() for value in re.split(r'[,;\s]+',settings.get('search_excluded_types','')) if value}
        category=settings.get('search_category','All files')
        intent=set()
        if category=='All files':
            if re.match(r'^(?:images?|photos?|pictures?)\s+(?:of\s+)?',query,re.I): intent={'image'}
            elif any(phrase in query.casefold() for phrase in ('where they talk','podcast','in the recording')): intent={'audio','video_audio'}
            elif re.search(r'\bin (?:a |the )?video\b',query,re.I): intent={'video','video_audio'}
        categories={'Photos / screenshots':{'image'},'Documents':{'text','pdf','document','spreadsheet','slides'},'Audio / video':{'audio','video','video_audio'},'Code':{'code'},'Archives / models':{'archive','model_metadata','filename'}}
        token_patterns=[re.compile(r'(?<!\w)'+re.escape(token)+r'(?!\w)') for token in tokens]
        identifier=bool(re.search(r'\d|[_:/-]',query))
        lowered=query.casefold()
        results=[]
        with self.connect() as db:
            # FTS handles Unicode token boundaries; literal scoring additionally retains punctuation in IDs.
            expression=' OR '.join('"'+token.replace('"','""')+'"' for token in tokens)
            bm25={rowid:-rank for rowid,rank in db.execute('SELECT rowid,bm25(words,1,2) FROM words WHERE words MATCH ?', (expression,))}
            exact=set(bm25)
            bm25_max=max(bm25.values(),default=1)
            for index,(rowid,path,kind,text,haystack,location) in enumerate(records):
                if intent and kind not in intent:continue
                if Path(path).suffix.lower() in excluded or category in categories and kind not in categories[category] or (excluded_files and os.path.normcase(os.path.abspath(path)) in excluded_files) or excluded_path(path,settings) or not safe(Path(Path(path).name)):
                    continue
                matches=sum(bool(pattern.search(haystack)) for pattern in token_patterns)
                literal=query.casefold() in haystack
                lexical=matches/len(tokens)
                semantic=float(scores[index]) if vector else 0
                meta=json.loads(location)
                group=kind if meta.get('embedding_source')=='media' else 'text'
                calibrated=semantic-modality_means.get(group,0)+.35 if vector else 0
                score=((2 if identifier else .65) if literal else 0)+((.35 if identifier else .15)*lexical if vector else lexical)+(.05 if rowid in exact else 0)+calibrated+.12*bm25.get(rowid,0)/max(bm25_max,1e-30)
                lowered=query.casefold()
                if vector:
                    if ('video' in lowered or 'clip' in lowered) and kind in ('video','video_audio'):
                        score+=.2
                    elif any(phrase in lowered for phrase in ('podcast','talk about','talks about','talking about','they talk','recording')) and kind in ('audio','video_audio'):
                        score+=.3
                    elif any(word in lowered for word in ('photo','picture','image')) and kind=='image':
                        score+=.2
                if matches or semantic>.12:
                    lower=text.casefold()
                    offset=min((lower.find(token) for token in tokens if token in lower),default=0)
                    snippet=' '.join(text[max(0,offset-50):offset+220].split())
                    result=SearchResult(score,path,snippet,kind=kind,query=query,**meta)
                    result.semantic_score=semantic
                    result.keyword_score=lexical
                    reasons=[]
                    if literal: reasons.append('Contains the exact search phrase')
                    if any(pattern.search(Path(path).name.casefold()) for pattern in token_patterns): reasons.append('Matches words in the filename')
                    if any(pattern.search(text.casefold()) for pattern in token_patterns): reasons.append('Matches text in the picture (OCR)' if kind=='image' else 'Matches words in the content')
                    if vector and semantic>.12: reasons.append({'image':'Matches the picture','video':'Matches this video frame','audio':'Matches this audio moment','video_audio':'Matches audio in the video','pdf':'Matches this scanned PDF page'}.get(kind,'Matches the meaning of this passage') if meta.get('embedding_source')=='media' else 'Matches the meaning of this passage')
                    result.reasons=reasons
                    result.lexical_match=bool(matches or literal)
                    results.append(result)
        if vector and results:
            floor=max(.3,max(result.semantic_score for result in results)-.13)
            results=[result for result in results if result.lexical_match or result.semantic_score>=floor]
        results.sort(key=lambda item:(-item.score,item.path,item.seconds,item.page))
        selected=[]
        for result in results:
            if any(old.path==result.path and old.kind==result.kind and old.page==result.page and old.line==result.line and (old.seconds<0 and result.seconds<0 or abs(old.seconds-result.seconds)<10) for old in selected):
                continue
            selected.append(result)
            if len(selected)==100:
                break
        logging.getLogger('vectordash').debug('Search completed in %.1f ms: %d results from %d segments',(time.perf_counter()-started)*1000,len(selected),len(records))
        return selected
