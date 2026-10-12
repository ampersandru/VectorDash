"""Persistent settings, incremental index, and search shared by the desktop UI."""
from contextlib import contextmanager
from scan_plan import ScanPlan, ScanPolicy
from dataclasses import dataclass
import numpy as np
from collections import OrderedDict
import re
import shutil
import stat as file_stat
import logging
import json
import os
from pathlib import Path
from sqlcipher3 import dbapi2 as sqlite3
from disk_index import open_database, account_key, migrate, change_encryption
import threading
import time
import urllib.request
import zipfile
from name_index import schema as names_schema, replace_catalog, sync_catalog, rows_for_search, source_mode, in_scope, normalized
import secrets
import hashlib
from parser_worker import isolated_extract
from security import MAGIC,seal,unseal,protect_secret,reveal_secret,contains_credentials,excluded_path
import xml.etree.ElementTree as ET
from vectordash_core import safe, read_text, embed, cosine, embedding_request, server_url, TEXT
from content import Segment, extract, project_files, code_segments, ocr_program, IMAGE, AUDIO, VIDEO

DEFAULTS = {
    'server_mode': 'Local', 'remote_url': '', 'remote_key': '', 'hf_token': '', 'github_token': '', 'remote_model_id': 'embeddinggemma-2',
    'projector': '', 'index_images': True, 'index_audio': True, 'index_video': True,
    'ocr_enabled': True, 'ocr_path': '', 'ocr_data': '', 'ocr_languages': 'eng', 'ffmpeg_path': '',
    'visual_detail': 'Balanced', 'pdf_visuals': False,
    'video_interval': 3, 'code_enabled': False, 'code_roots': [],
    'programs_enabled': False, 'roots': [], 'mode': 'CPU / system RAM', 'server': '', 'model': '', 'device': '',
    'search_names': False, 'search_mode': 'Semantic', 'source_modes': {}, 'search_sources': None, 'new_source_mode': 'full',
    'accent_color': '#8573ff', 'popup_tone': '#8a73a8', 'popup_transparency': 25, 'popup_gradient': 20, 'sidebar_collapsed': False,
    'semantic': False, 'theme': 'System', 'hotkey': 'Ctrl+Shift+Space',
    'startup': False, 'start_hidden': True, 'close_behavior': 'Hide to tray',
    'minimize_behavior': 'Taskbar', 'auto_model': True,
    'schedule_mode': 'Manual', 'schedule_time': '02:00', 'schedule_days': [0,1,2,3,4,5,6],
    'idle_minutes': 5, 'cpu_limit': 25, 'gpu_limit': 20, 'check_cpu': True,
    'check_gpu': False, 'check_user_idle': True, 'ac_only': True, 'idle_interval': 60, 'last_success': '',
    'mcp_lan': False, 'mcp_enabled': False, 'mcp_port': 8790, 'mcp_auth': True, 'mcp_token': '',
    'performance_profile': 'Balanced', 'encrypt_index': True, 'exclude_windows_churn': False, 'exclude_sensitive': True, 'excluded_folders': [], 'index_excluded_types': '', 'search_excluded_types': '', 'search_excluded_files': [],
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
            loaded=json.loads(self.path.read_text(encoding='utf-8-sig'))
            self.values.update(loaded)
            if 'search_mode' not in loaded:self.values['search_mode']='Both' if loaded.get('search_names') else 'Semantic'
            self.values['search_names']=False
        for key in ('remote_key','mcp_token','hf_token','github_token'):
            self.values[key]=reveal_secret(self.values.get(key,''))
        if not self.values['mcp_token']: self.values['mcp_token']=secrets.token_urlsafe(32)
    def save(self):
        temp = self.path.with_suffix('.tmp')
        stored=dict(self.values)
        for key in ('remote_key','mcp_token','hf_token','github_token'): stored[key]=protect_secret(stored.get(key,''))
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
        if self.kind in ('calculator','calculator_error'):return 'Calculator'
        if self.kind=='settings':return 'Windows Settings'
        if self.kind=='application':return 'Program · '+self.snippet
        return Segment(kind=self.kind,page=self.page,seconds=self.seconds,line=self.line,end_line=self.end_line,symbol=self.symbol).location()

class Engine:
    def __init__(self, home, policy=None, defer=False):
        from programs import ProgramCatalog
        self.programs=ProgramCatalog(home)
        self.db = Path(home)/'index.sqlite'
        if not self.db.exists():
            for suffix in ('.legacy.dpapi','.legacy.sqlite','.encryption-backup'):
                backup=self.db.with_suffix(suffix)
                if backup.exists():backup.replace(self.db);break
        self.protected=os.name=='nt'
        self.storage_lock=threading.RLock()
        self.storage_version=0
        self.key=account_key(home) if self.protected else None
        self.encrypted=self.protected and (policy or {}).get('encrypt_index',True)
        if not self.encrypted:self.key=None
        self.ready=threading.Event();self.initialization_error=None;self.preparing=False
        self.cancel = threading.Event()
        self.paused = threading.Event()
        self.inference = threading.Lock()
        self.query_waiting = threading.Event()
        self.search_cache_lock=threading.RLock()
        self.search_cache={}
        self.query_vectors=OrderedDict()
        from search_resources import VectorCache
        self.vector_cache=VectorCache()
        self.performance_profile=(policy or {}).get('performance_profile','Balanced')
        self.result_cache=OrderedDict();self.result_cache_bytes=0
        self.preparing=self.needs_conversion()
        if defer and not self.preparing and self.db.exists():
            with open_database(self.db,self.key,True) as probe:
                self.preparing=not probe.execute("SELECT 1 FROM sqlite_master WHERE name='names_base'").fetchone()
            probe.close()
        if defer and self.preparing:
            threading.Thread(target=self.initialize,args=(policy,),daemon=True).start()
        else:self.initialize(policy)

    def needs_conversion(self):
        if not self.db.exists():return False
        with self.db.open('rb') as stored:header=stored.read(16)
        return header.startswith(MAGIC) or (header.startswith(b'SQLite format 3')==self.encrypted)

    def initialize(self,policy=None):
        self.initializing_thread=threading.get_ident()
        try:
            if self.needs_conversion():
                logging.getLogger('vectordash').info('Converting stored index to bounded disk storage')
                with self.db.open('rb') as stored:legacy=stored.read(len(MAGIC))==MAGIC
                if legacy:migrate(self.db,self.key,lambda text:logging.getLogger('vectordash').info(text))
                else:
                    with self.db.open('rb') as stored:plain=stored.read(16)==b'SQLite format 3\0'
                    old_key=None if plain else account_key(self.db.parent)
                    change_encryption(self.db,old_key,self.key)
            with self.connect(readonly=True) as existing:
                tables={row[0]:row[1] for row in existing.execute("SELECT name,sql FROM sqlite_master")}
                ready=all(name in tables for name in ('files','metadata','segments','segments_path','names','name_words','words','segment_vectors','vectors_update','vectors_delete','vectors_path','names_base'))
                if ready:ready={'mtime','size'}<={row[1] for row in existing.execute('PRAGMA table_info(files)')} and {'base','mtime','size','index_mode'}<={row[1] for row in existing.execute('PRAGMA table_info(names)')} and "content='names'" in tables['name_words']
            if not ready:self.initialize_schema()
            if not policy or policy.get('exclude_sensitive',True):
                with self.connect(readonly=True) as db:metadata=dict(db.execute('SELECT key,value FROM metadata'))
                try:checked=self.protected and metadata.get('security_version')=='1' and json.loads(metadata.get('content_options','{}')).get('exclude_sensitive') is True
                except (ValueError,TypeError):checked=False
                if not checked:self.remove_sensitive_legacy_content()
            self.index_stats()
        except BaseException as error:
            self.initialization_error=error
            logging.getLogger('vectordash').error('Could not prepare stored index: %s',error)
            if not self.preparing:raise
        finally:self.preparing=False;self.ready.set()

    def wait_ready(self):
        if getattr(self,'initializing_thread',None)!=threading.get_ident():self.ready.wait()
        if self.initialization_error:raise self.initialization_error

    def initialize_schema(self):
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
            db.execute('CREATE TABLE IF NOT EXISTS segment_vectors(id INTEGER PRIMARY KEY,namespace TEXT,kind TEXT,channel TEXT,dimension INTEGER,vector BLOB,path TEXT)')
            if 'path' not in {row[1] for row in db.execute('PRAGMA table_info(segment_vectors)')}:
                db.execute('ALTER TABLE segment_vectors ADD COLUMN path TEXT')
                db.execute('UPDATE segment_vectors SET path=(SELECT path FROM segments WHERE rowid=segment_vectors.id)')
            db.execute('CREATE INDEX IF NOT EXISTS vectors_path ON segment_vectors(namespace,dimension,path,channel)')
            db.execute('CREATE INDEX IF NOT EXISTS vectors_media ON segment_vectors(path,channel)')
            db.execute('CREATE INDEX IF NOT EXISTS vectors_namespace ON segment_vectors(namespace,dimension)')
            db.execute('CREATE TRIGGER IF NOT EXISTS vectors_update AFTER UPDATE OF vector,location,kind,namespace ON segments BEGIN DELETE FROM segment_vectors WHERE id=old.rowid; END')
            db.execute('CREATE TRIGGER IF NOT EXISTS vectors_delete AFTER DELETE ON segments BEGIN DELETE FROM segment_vectors WHERE id=old.rowid; END')
            names_schema(db)
            db.execute("CREATE VIRTUAL TABLE IF NOT EXISTS words USING fts5(path,text,tokenize='unicode61')")
            # Preserve readable legacy results until a successful replacement.
            if not db.execute('SELECT 1 FROM segments LIMIT 1').fetchone():
                db.execute("INSERT INTO segments SELECT path,'files','text',text,vector,'{}' FROM files")
                db.execute('INSERT INTO words(rowid,path,text) SELECT rowid,path,text FROM segments')
            self.sync_vectors(db)

    def remove_sensitive_legacy_content(self):
        # Stream the upgrade filter; do not accumulate sensitive paths/text.
        count=0
        with self.connect() as db:
            for path,text in db.execute('SELECT path,text FROM segments'):
                if safe(Path(Path(path).name)) and not contains_credentials(text):continue
                db.execute('DELETE FROM words WHERE path=?',(path,))
                db.execute('DELETE FROM segments WHERE path=?',(path,))
                db.execute('DELETE FROM files WHERE path=?',(path,));count+=1
            if count:db.execute('INSERT OR REPLACE INTO metadata VALUES(?,?)',('index_counts',json.dumps(self.count_index(db))))
        return count

    @staticmethod
    def count_index(db):
        metadata=dict(db.execute('SELECT key,value FROM metadata'))
        modes=json.loads(metadata.get('source_modes','{}'))
        policy={'roots':list(modes),'source_modes':modes}
        full=vectors=0
        quick=db.execute("SELECT count(*) FROM names WHERE kind='file' AND index_mode='names'").fetchone()[0]
        for path,has_vector,has_code in db.execute("SELECT path,max(CASE WHEN vector NOT IN ('','[]','null') THEN 1 ELSE 0 END),max(CASE WHEN namespace='code' THEN 1 ELSE 0 END) FROM segments GROUP BY path"):
            in_quick=db.execute("SELECT 1 FROM names WHERE path=? AND kind='file' AND index_mode='names'",(path,)).fetchone()
            if source_mode(path,policy)=='names' and not has_vector and not has_code:
                if not in_quick:quick+=1
            else:
                full+=1
                if in_quick:quick-=1
                if has_vector:vectors+=1
        folders=db.execute("SELECT count(*) FROM names WHERE kind='folder'").fetchone()[0]
        return {'total':full+quick,'full':full,'quick':quick,'vectors':vectors,'without_vectors':full-vectors,'folders':folders}

    def index_stats(self):
        if self.preparing and getattr(self,'initializing_thread',None)!=threading.get_ident():return getattr(self,'_index_stats',dict(total=0,full=0,quick=0,vectors=0,without_vectors=0,folders=0))
        version=self.storage_version if self.protected else tuple((path.stat().st_mtime_ns,path.stat().st_size) if path.exists() else None for path in (self.db,Path(str(self.db)+'-wal')))
        if getattr(self,'_index_stats_version',None)==version:return dict(self._index_stats)
        with self.connect(readonly=True) as db:
            row=db.execute("SELECT value FROM metadata WHERE key='index_counts'").fetchone()
            stats=json.loads(row[0]) if row else self.count_index(db)
        self._index_stats_version=version;self._index_stats=stats
        return dict(stats)

    def source_stats(self,settings,fill_missing=False):
        self.wait_ready();version=self.storage_version
        if getattr(self,'_source_stats_version',None)==version:
            activity=dict(self._source_stats)
        else:
            with self.connect(readonly=True) as db:row=db.execute("SELECT value FROM metadata WHERE key='source_counts'").fetchone()
            activity=json.loads(row[0]) if row else {}
        if fill_missing:
            with self.connect(readonly=True) as db:
                for root in settings.get('roots',[]):
                    key=normalized(root)
                    if key in activity:continue
                    # Old indexes have no addition history. Count in a worker;
                    # new additions are tracked from the next successful update.
                    prefix=str(Path(root)).rstrip(os.sep)+os.sep;upper=prefix[:-1]+chr(ord(prefix[-1])+1)
                    count=db.execute("SELECT count(*) FROM names WHERE kind='file' AND (path=? OR (path>=? AND path<?))",(str(Path(root)),prefix,upper)).fetchone()[0]
                    activity[key]={'files':count,'events':[]}
        if self.storage_version!=version:return self.source_stats(settings,fill_missing)
        self._source_stats=activity;self._source_stats_version=version
        now=time.time()
        return {normalized(root):{'files':activity.get(normalized(root),{}).get('files'), 'added_hour':sum(event[1] for event in activity.get(normalized(root),{}).get('events',[]) if event[0]>=now-3600)} for root in settings.get('roots',[])}

    def indexed_file_count(self):
        return self.index_stats()['total']

    def indexed_folder_count(self):
        return self.index_stats()['folders']

    def cached_index(self,namespace):
        self.wait_ready()
        # The only reusable index state is small metadata, never documents,
        # filenames, locations, or a complete embedding matrix.
        with self.search_cache_lock:
            if namespace not in self.search_cache:
                with self.connect(readonly=True) as db:
                    metadata=dict(db.execute('SELECT key,value FROM metadata'))
                    metadata['_has_vectors']=bool(db.execute('SELECT 1 FROM segment_vectors WHERE namespace=? LIMIT 1',(namespace,)).fetchone())
                self.search_cache[namespace]=([],{},metadata)
            return self.search_cache[namespace]

    def clear_query_cache(self):
        with self.search_cache_lock:
            self.query_vectors.clear();self.result_cache.clear();self.result_cache_bytes=0

    @contextmanager
    def connect(self,readonly=False,preserve_content=False,checkpoint=None,search=False):
        self.wait_ready()
        from search_resources import profile
        cache_kib=profile(self.performance_profile)['pages']*1024 if search else None
        connection=open_database(self.db,self.key,readonly,cache_kib)
        if not readonly:connection.execute('PRAGMA journal_mode=WAL')
        changes=connection.total_changes
        schema=connection.execute('PRAGMA schema_version').fetchone()[0]
        try:
            with connection:
                yield connection
                if not readonly and connection.total_changes!=changes and connection.execute("SELECT 1 FROM sqlite_master WHERE name='segment_vectors'").fetchone():self.sync_vectors(connection)
                if not readonly and checkpoint:checkpoint()
            if not readonly and (connection.total_changes!=changes or connection.execute('PRAGMA schema_version').fetchone()[0]!=schema):
                with self.storage_lock:old_version=self.storage_version;self.storage_version+=1
                if preserve_content:self.vector_cache.advance(old_version,self.storage_version)
                if not preserve_content:
                    with self.search_cache_lock:self.search_cache.clear();self.result_cache.clear();self.result_cache_bytes=0
                    self.vector_cache.clear()
        finally:connection.close()

    @staticmethod
    def sync_vectors(db):
        db.execute('DELETE FROM segment_vectors WHERE id NOT IN (SELECT rowid FROM segments)')
        for rowid,namespace,kind,stored,location,path in db.execute("SELECT rowid,namespace,kind,vector,location,path FROM segments WHERE vector NOT IN ('','[]','null') AND rowid NOT IN (SELECT id FROM segment_vectors)"):
            vector=np.asarray(json.loads(stored),dtype=np.float32)
            if not vector.size:continue
            vector/=max(float(np.linalg.norm(vector)),1e-30)
            channel=kind if json.loads(location).get('embedding_source')=='media' else 'text'
            db.execute('INSERT INTO segment_vectors VALUES(?,?,?,?,?,?,?)',(rowid,namespace,kind,channel,len(vector),vector.tobytes(),path))

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

    def index(self, settings, progress=lambda text: None, condition=lambda: True, force=False, file_progress=lambda state: None):
        settings=dict(settings)
        policy=ScanPolicy(settings)
        mode_for=lambda path:policy.mode(path)
        full_sources=any(mode_for(root)!='names' and not policy.excluded(root) for root in settings['roots']) or settings.get('code_enabled') and settings.get('code_roots')
        if not full_sources:
            settings['semantic']=False;settings['ocr_enabled']=False
        signature=model_signature(settings)
        if settings['semantic']:verify_server_model(settings)
        content_options={key:settings.get(key) for key in ('visual_detail','pdf_visuals','index_images','index_audio','index_video','ocr_enabled','ocr_path','ocr_data','ocr_languages','video_interval','code_enabled','code_roots','exclude_sensitive','excluded_folders','index_excluded_types')}
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

        checked_media=False
        completed=resumed=0
        plan=ScanPlan(self.db.parent,encrypted=self.encrypted)
        candidates=plan
        try:
            cache_dir=self.db.parent/'index-resume'
            scan_started=time.perf_counter()
            last_scan_report=scan_started
            resolved_roots={namespace:[Path(root).resolve() for root in roots] for namespace,roots in [('files',settings['roots']),('code',settings.get('code_roots',[]))]}
            def queue(path,namespace,stat=None):
                self.checkpoint(wait_query=False)
                nonlocal skipped
                path=os.path.abspath(path)
                if policy.excluded(path):return
                if namespace=='code':
                    roots=resolved_roots[namespace]
                    relative=next((Path(path).relative_to(root) for root in roots if Path(path).is_relative_to(root)),Path(path).name)
                    if not safe(Path(relative)):return
                try:stat=stat if stat is not None else os.lstat(path)
                except OSError as error:
                    skipped+=1;logging.getLogger('vectordash').warning('Skipped inaccessible file: %s: %s',path,error);return
                if file_stat.S_ISLNK(stat.st_mode) or getattr(stat,'st_file_attributes',0)&(0x2|0x400|0x1000|0x40000|0x400000):return
                plan.queue(path,namespace,stat,mode_for(path) if namespace=='files' else 'full')
            def scan_source(source):
                nonlocal skipped,last_scan_report
                root=Path(os.path.abspath(source))
                if root.is_file():
                    if safe(Path(root.name)) and not excluded_path(root,settings):queue(root,'files')
                    return
                pending=[root]
                plan.folder(root)
                while pending:
                    self.checkpoint(wait_query=False)
                    base=pending.pop()
                    try:
                        with os.scandir(base) as entries:
                            for entry in entries:
                                self.checkpoint(wait_query=False)
                                path=entry.path
                                if not policy.safe_name(entry.name) or policy.excluded(path):continue
                                try:
                                    stat=entry.stat(follow_symlinks=False)
                                    if file_stat.S_ISLNK(stat.st_mode) or getattr(stat,'st_file_attributes',0)&(0x2|0x400|0x1000|0x40000|0x400000):continue
                                    if entry.is_dir(follow_symlinks=False):
                                        pending.append(path)
                                        plan.folder(path)
                                    elif entry.is_file(follow_symlinks=False):queue(path,'files',stat)
                                except OSError as error:
                                    skipped+=1
                                    logging.getLogger('vectordash').warning('Skipped inaccessible entry: %s: %s',path,error)
                                now=time.perf_counter()
                                if now-last_scan_report>=.2:
                                    last_scan_report=now
                                    file_progress({'phase':'scanning','total':len(candidates),'needs_index':0,'completed':0,'remaining':0,'reused':0})
                                    time.sleep(.001)
                    except OSError as error:
                        skipped+=1
                        logging.getLogger('vectordash').warning('Skipped inaccessible folder: %s',error)
            file_progress({'phase':'scanning','total':0,'needs_index':0,'completed':0,'remaining':0,'reused':0})
            with self.connect(readonly=True) as db:
                metadata=dict(db.execute('SELECT key,value FROM metadata'))
                plan.load_files(db.execute('SELECT path,mtime,size FROM files'))
                plan.load_previous(db.execute('SELECT path,kind,mtime,size,index_mode FROM names'))
                plan.load_segments(db.execute('SELECT DISTINCT path,namespace FROM segments'))
                compatible=metadata.get('security_version')=='1' and metadata.get('signature')==signature and metadata.get('content_options')==configuration and metadata.get('segments_version')=='4' and not force
            reasons=[]
            if force:reasons.append('full rebuild requested')
            elif plan.db.execute('SELECT 1 FROM old_files LIMIT 1').fetchone():
                for key,expected in [('security_version','1'),('signature',signature),('content_options',configuration),('segments_version','4')]:
                    if metadata.get(key)!=expected:reasons.append(key+' changed')
            if reasons:logging.getLogger('vectordash').info('Index refresh reason: %s',', '.join(reasons))
            def covered_by(absolute,parent):
                if not absolute.is_relative_to(parent) or not safe(absolute.relative_to(parent)):return False
                current=absolute
                while current!=parent:
                    try:
                        stat=current.lstat()
                        if file_stat.S_ISLNK(stat.st_mode) or getattr(stat,'st_file_attributes',0)&(0x2|0x400|0x1000|0x40000|0x400000):return False
                    except OSError:return False
                    current=current.parent
                return True
            scanned_roots=[]
            for source in sorted(settings['roots'],key=lambda root:len(os.path.abspath(root))):
                if not Path(source).exists() or Path(source).is_symlink() or Path(source).is_junction():
                    raise ValueError(f'Source unavailable: {source}; previous index retained')
                absolute=Path(os.path.abspath(source))
                covered=any(covered_by(absolute,parent) for parent in scanned_roots)
                if not covered:
                    if not policy.excluded(absolute):scan_source(source)
                    scanned_roots.append(absolute)
            if settings.get('code_enabled'):
                for root in settings.get('code_roots',[]):
                    if not Path(root).is_dir() or Path(root).is_symlink() or Path(root).is_junction():
                        raise ValueError(f'Project unavailable: {root}; previous index retained')
                    if policy.excluded(root):continue
                    for path in project_files(root):
                        from content import CODE
                        if path.suffix.lower() in CODE:
                            queue(path,'code')
            previous_modes=json.loads(metadata.get('source_modes','{}'))
            old_settings=dict(settings,roots=list(previous_modes) or settings['roots'],source_modes=previous_modes)
            old_policy=ScanPolicy(old_settings)
            def unchanged(path,namespace,stat):
                old=plan.old_file(path)
                mode=mode_for(path) if namespace=='files' else 'full'
                old_mode=old_policy.mode(path) if namespace=='files' else 'full'
                valid=(metadata.get('security_version')=='1' and not force) if mode=='names' else compatible
                if mode=='names':
                    saved=plan.old_quick(path) or (old[2:] if old and old_mode=='names' else None)
                    return bool(valid and saved==(stat.st_mtime_ns,stat.st_size))
                return bool(valid and mode==old_mode and old and old[2:]==(stat.st_mtime_ns,stat.st_size) and plan.has_segment(path,namespace))
            valid_quick=metadata.get('security_version')=='1' and not force
            plan.mark_quick(valid_quick)
            if valid_quick:
                for path,old in plan.files():
                    if mode_for(path)=='names' and old_policy.mode(path)=='names':plan.mark_legacy_quick(path,old[2],old[3])
            for path,namespace,stat in plan.entries(full_only=True):plan.mark(path,namespace,not unchanged(path,namespace,stat))
            needed=plan.needed()
            retained=len(candidates)-needed
            last_report=0.0
            reported_completed=0
            def report(current='',final=False):
                nonlocal last_report,reported_completed
                now=time.perf_counter()
                if not final and now-last_report<.15 and not (reported_completed==0 and completed>0):return
                last_report=now;reported_completed=completed
                file_progress({'phase':'indexing','total':len(candidates),'needs_index':needed,'completed':completed,'remaining':max(0,needed-completed),'reused':retained,'resumed':resumed,'skipped':skipped,'current':current})
            report()
            logging.getLogger('vectordash').debug('Source count/planning: %.1f ms; %d eligible files; %d changed',(time.perf_counter()-scan_started)*1000,len(candidates),needed)
            progress(f'{len(candidates):,} files found · {needed:,} need indexing · {retained:,} unchanged')
            validation_started=time.perf_counter()
            plan.db.set_progress_handler(lambda: int(self.cancel.is_set()),10000)
            keys_changed=plan.full_keys_changed()
            self.checkpoint(wait_query=False)
            logging.getLogger('vectordash').debug('Validate retained files: %.1f ms',(time.perf_counter()-validation_started)*1000)
            if needed==0 and not keys_changed and not plan.names_changed():
                with self.connect(readonly=True) as reader:total=reader.execute('SELECT count(*) FROM segments').fetchone()[0]
                logging.getLogger('vectordash').info('Index unchanged; retained completed disk index and metadata cache')
                return f'{len(candidates)} files indexed · {retained} reused (unchanged) · 0 resumed · {skipped} skipped · {total} segments'
            content_changed=bool(plan.full_needed() or keys_changed)
            with self.connect(preserve_content=not content_changed,checkpoint=lambda:self.checkpoint(wait_query=False)) as db:
                plan.db.commit()
                literal="x'"+plan.key.hex()+"'" if plan.key else ''
                db.execute('ATTACH DATABASE ? AS scan KEY ?',(str(plan.path),literal))
                db.execute('PRAGMA scan.cache_size=-8192')
                db.execute('CREATE TABLE scan.replacement(path TEXT PRIMARY KEY,text TEXT,vector TEXT,mtime INTEGER,size INTEGER)')
                db.execute('CREATE TABLE scan.new_segments(path TEXT,namespace TEXT,kind TEXT,text TEXT,vector TEXT,location TEXT)')
                db.execute('CREATE INDEX scan.new_segments_path ON new_segments(path,namespace)')
                def add(path,namespace,scanned_stat):
                    nonlocal count,reused,skipped,segment_count,checked_media,completed,resumed,needed,retained
                    self.checkpoint(condition)
                    path=Path(path)
                    key=(str(path),namespace)
                    pending=True
                    try:
                        stat=scanned_stat if namespace=='files' and mode_for(path)=='names' else path.lstat()
                        roots=sorted(settings.get('code_roots',[]) if namespace=='code' else settings['roots'],key=len,reverse=True)
                        relative=next((path.relative_to(Path(root)) for root in roots if path.is_relative_to(Path(root))),Path(path.name))
                        if not safe(relative) or excluded_path(path,settings) or file_stat.S_ISLNK(stat.st_mode) or getattr(stat,'st_file_attributes',0)&(0x2|0x400|0x1000|0x40000|0x400000):
                            return
                        old=plan.old_file(path)
                        pending=not unchanged(path,namespace,stat)
                        if pending!=plan.planned(path,namespace):
                            adjustment=1 if pending else -1
                            needed+=adjustment;retained-=adjustment
                        if namespace=='files' and mode_for(path)=='names':
                            if pending:count+=1;completed+=1
                            else:reused+=1
                            report(path.name)
                            return
                        if unchanged(path,namespace,stat):
                            reused+=1
                        else:
                            fingerprint=json.dumps([str(path),namespace,stat.st_mtime_ns,stat.st_size,signature,configuration,'4'],sort_keys=True)
                            cache_file=cache_dir/(hashlib.sha256(fingerprint.encode()).hexdigest()+'.cache')
                            if cache_file.is_file() and not force:
                                try:
                                    stored=cache_file.read_bytes()
                                    cached=json.loads((unseal(stored) if self.protected else stored).decode())
                                    if cached['fingerprint']!=fingerprint:raise ValueError('Resume fingerprint mismatch')
                                    for row in cached['segments']:
                                        db.execute('INSERT INTO new_segments VALUES(?,?,?,?,?,?)',row)
                                    db.execute('INSERT OR REPLACE INTO replacement VALUES(?,?,?,?,?)',cached['file'])
                                    resumed+=1;count+=1;completed+=1;report(path.name)
                                    return
                                except Exception:
                                    db.execute('DELETE FROM new_segments WHERE path=? AND namespace=?',key)
                                    logging.getLogger('vectordash').warning('Unreadable resume entry; processing file again: %s',path)
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
                                supported=IMAGE|AUDIO|VIDEO|{'.pdf','.docx','.xlsx','.pptx','.zip','.safetensors'}
                                segments=extract(path,settings,read_text,lambda:self.checkpoint(condition)) if path.suffix.lower() in TEXT else isolated_extract(path,settings,lambda:self.checkpoint(condition)) if path.suffix.lower() in supported else [Segment(kind='filename')]
                            file_text=[]
                            file_segments=0
                            for segment in segments:
                                self.checkpoint(condition)
                                if settings.get('exclude_sensitive',True) and contains_credentials(segment.text):
                                    raise ValueError('Content excluded: potential credentials detected')
                                vectors=self.infer(segment,path,condition,settings) if settings['semantic'] and segment.kind!='filename' else [[]]
                                for vector_number,vector in enumerate(vectors):
                                    location=json.loads(segment.metadata())
                                    location['embedding_source']='media' if segment.media and vector_number==0 else 'text'
                                    db.execute('INSERT INTO new_segments VALUES(?,?,?,?,?,?)',(str(path),namespace,segment.kind,segment.text,json.dumps(vector),json.dumps(location)))
                                segment_count+=1
                                file_segments+=1
                                kinds[segment.kind]=kinds.get(segment.kind,0)+1
                                if file_segments%10==0:
                                    progress(f'{path.name} · {file_segments} segments · {segment.location()}')
                                if len(file_text)<10:file_text.append(segment.text[:100])
                            if not file_segments:
                                db.execute('INSERT INTO new_segments VALUES(?,?,?,?,?,?)',(str(path),namespace,'filename','','[]','{}'))
                            db.execute('INSERT OR REPLACE INTO replacement VALUES(?,?,?,?,?)',(str(path),' '.join(file_text[:10]),'[]',stat.st_mtime_ns,stat.st_size))
                            # Completed files survive cancellation/time limits without exposing
                            # a partial index. Windows checkpoint contents are account-encrypted.
                            checkpoint_rows=db.execute('SELECT count(*) FROM new_segments WHERE path=? AND namespace=?',key).fetchone()[0]
                            cached={'fingerprint':fingerprint,'file':[str(path),' '.join(file_text[:10]),'[]',stat.st_mtime_ns,stat.st_size],'segments':list(db.execute('SELECT * FROM new_segments WHERE path=? AND namespace=?',key)) if checkpoint_rows<=128 else []}
                            try:
                                if checkpoint_rows>128:raise OSError('Long-file resume checkpoint omitted to bound memory')
                                cache_dir.mkdir(exist_ok=True)
                                payload=json.dumps(cached,ensure_ascii=False).encode()
                                temporary=cache_file.with_suffix('.tmp')
                                temporary.write_bytes(seal(payload) if self.protected else payload)
                                temporary.replace(cache_file)
                            except OSError:
                                logging.getLogger('vectordash').warning('Could not save indexing resume checkpoint',exc_info=True)

                        count+=int(pending)
                    except Exception as error:
                        if isinstance(error,(InterruptedError,sqlite3.Error)):
                            raise
                        db.execute('DELETE FROM new_segments WHERE path=? AND namespace=?',key)
                        skipped+=1

                        if len(warnings)<100:warnings.append(f'{path}: {error}')
                        logging.getLogger('vectordash').warning('Skipped %s: %s',path,error)
                        logging.getLogger('vectordash').debug('File failure details: %s',path,exc_info=True)
                        progress(f'Skipped {path.name}: {error}')
                        db.execute('INSERT INTO new_segments VALUES(?,?,?,?,?,?)',(str(path),namespace,'filename','','[]','{}'))
                        db.execute('INSERT OR REPLACE INTO replacement VALUES(?,?,?,?,?)',(str(path),'','[]',scanned_stat.st_mtime_ns,scanned_stat.st_size))
                    if pending:
                        completed+=1
                    report(path.name)
                quick_changed,quick_reused=plan.quick_counts()
                count+=quick_changed;reused+=quick_reused;completed+=quick_changed
                if content_changed:
                    for path,namespace,stat in plan.entries(full_only=True):add(path,namespace,stat)
                else:reused+=plan.full_count()
                report(final=True)
                self.checkpoint(condition)
                progress("Saving the completed index…")
                sync_catalog(db,plan.changes(),plan.removals())
                now=int(time.time())
                activity=json.loads(metadata.get('source_counts','{}'))
                for root,counts in plan.source_counts(settings['roots']).items():
                    events=[event for event in activity.get(root,{}).get('events',[]) if event[0]>=now-3600]
                    if counts['new']:events.append([now,counts['new']])
                    activity[root]={'files':counts['files'],'events':events}
                activity={normalized(root):activity[normalized(root)] for root in settings['roots']}
                db.execute('INSERT OR REPLACE INTO metadata VALUES(?,?)',('source_counts',json.dumps(activity)))
                db.execute('INSERT OR REPLACE INTO metadata VALUES(?,?)',('source_modes',json.dumps({normalized(root):mode_for(root) for root in settings['roots']},sort_keys=True)))
                if content_changed:
                    retained="EXISTS(SELECT 1 FROM scan.candidates c JOIN scan.catalog n USING(key) WHERE n.path=segments.path AND c.namespace=segments.namespace AND (c.mode!='names' OR c.namespace='code'))"
                    replaced="EXISTS(SELECT 1 FROM scan.new_segments s WHERE s.path=segments.path AND s.namespace=segments.namespace)"
                    removed='NOT '+retained+' OR '+replaced
                    db.execute('DELETE FROM words WHERE rowid IN (SELECT rowid FROM segments WHERE '+removed+')')
                    db.execute('DELETE FROM segment_vectors WHERE id IN (SELECT rowid FROM segments WHERE '+removed+')')
                    db.execute('DELETE FROM segments WHERE '+removed)
                    last=db.execute('SELECT coalesce(max(rowid),0) FROM segments').fetchone()[0]
                    db.execute('INSERT INTO segments SELECT * FROM scan.new_segments')
                    db.execute("INSERT INTO words(rowid,path,text) SELECT rowid,path,text FROM segments WHERE rowid>? AND kind!='filename'",(last,))
                    db.execute("DELETE FROM files WHERE NOT EXISTS(SELECT 1 FROM scan.candidates c JOIN scan.catalog n USING(key) WHERE n.path=files.path AND (c.mode!='names' OR c.namespace='code'))")
                    db.execute('INSERT OR REPLACE INTO files SELECT * FROM scan.replacement')
                    self.sync_vectors(db)
                for key,value in [('signature',signature),('model',settings['model'] if settings['semantic'] else ''),('segments_version','4'),('security_version','1'),('content_options',configuration),('warnings',json.dumps(warnings,ensure_ascii=False))]:
                    db.execute('INSERT OR REPLACE INTO metadata VALUES(?,?)',(key,value))
                db.execute('INSERT OR REPLACE INTO metadata VALUES(?,?)',('index_counts',json.dumps(self.count_index(db))))
                total=db.execute('SELECT count(*) FROM segments').fetchone()[0]
                summary=', '.join(f'{n} {kind}' for kind,n in db.execute('SELECT kind,count(*) FROM segments GROUP BY kind'))
            # A successful replacement no longer needs checkpoints, including those
            # left by older settings or files removed while a run was interrupted.
            if cache_dir.is_dir() and not cache_dir.is_symlink() and not cache_dir.is_junction():
                for cache_file in cache_dir.glob('*.cache'):
                    if re.fullmatch(r'[0-9a-f]{64}\.cache',cache_file.name):
                        try:cache_file.unlink(missing_ok=True)
                        except OSError:pass
            plan.close()
            self.index_stats()  # Prepare counts on the worker, not the UI thread.
            result=f'{count} files indexed · {reused} reused (unchanged) · {resumed} resumed · {skipped} skipped · {total} segments ({summary})'
            if warnings:
                result+=' · '+warnings[0]+(f' (+{len(warnings)-1} warnings)' if len(warnings)>1 else '')
            return result

        except sqlite3.OperationalError as error:
            if self.cancel.is_set():raise InterruptedError('Indexing stopped; previous completed index retained') from error
            raise
        finally:
            plan.close()

    def warm_search_resources(self):
        self.wait_ready()
        from search_resources import profile
        if not profile(self.performance_profile)['vectors']:return
        with self.connect(readonly=True,search=True) as db:
            db.execute('BEGIN')
            for namespace,dimension in db.execute('SELECT DISTINCT namespace,dimension FROM segment_vectors'):
                self.vector_cache.get(self,db,namespace,dimension,self.performance_profile)

    def configure_performance(self,name):
        from search_resources import PROFILES
        name=name if name in PROFILES else 'Balanced'
        if name!=self.performance_profile:
            self.performance_profile=name;self.vector_cache.clear()
            with self.search_cache_lock:self.result_cache.clear();self.result_cache_bytes=0

    def search(self,query,settings):
        from quick_actions import results as action_results, exclusive
        actions=action_results(query,settings)
        if exclusive(query,settings) or any(r.kind.startswith('calculator') for r in actions):return actions
        self.configure_performance(settings.get('performance_profile','Balanced'))
        key=(self.storage_version,self.programs.scanned,query.strip(),hashlib.sha256(json.dumps(settings,sort_keys=True,default=str).encode()).digest())
        with self.search_cache_lock:
            if key in self.result_cache:
                import copy
                self.result_cache.move_to_end(key)
                return copy.deepcopy(self.result_cache[key][0])
        results=self.search_uncached(query,settings)
        import copy
        stored=copy.deepcopy(results)
        size=sum(len(json.dumps(vars(result),default=str).encode()) for result in stored)
        with self.search_cache_lock:
            previous=self.result_cache.pop(key,None)
            if previous:self.result_cache_bytes-=previous[1]
            while self.result_cache and (self.result_cache_bytes+size>4*1024*1024 or len(self.result_cache)>=32):
                _,old=self.result_cache.popitem(last=False);self.result_cache_bytes-=old[1]
            if size<=4*1024*1024:
                self.result_cache[key]=(stored,size);self.result_cache_bytes+=size
        return results

    def search_uncached(self,query,settings):
        from quick_actions import results as action_results, exclusive
        actions=action_results(query,settings)
        if exclusive(query,settings) or any(r.kind.startswith('calculator') for r in actions):return actions
        programs=actions+self.programs.search(query,settings)
        if query.lower().startswith('app:') or settings.get('search_category')=='Programs':return programs
        try:files=self.search_files(query,settings)
        except Exception:
            if not programs:raise
            logging.getLogger('vectordash').warning('File search failed; program results remain available.',exc_info=True)
            files=[]
        return sorted(programs+files,key=lambda r:(-r.score,r.path))[:100]

    def search_files(self,query,settings):
        mode='Both' if settings.get('search_names') else settings.get('search_mode','Semantic')
        if mode=='Names':return self.search_filename(query,settings)
        content=self.search_content(query,dict(settings,search_names=bool(settings.get('search_names'))))
        if mode!='Both':return content
        names=self.search_filename(query,settings)
        paths={item.path for item in content}
        for name in names:
            if name.path in paths:
                for result in content:
                    if result.path==name.path:
                        result.score+=min(.35,name.score*.15)
                        result.reasons.extend(reason for reason in name.reasons if reason not in result.reasons)
            else:content.append(name)
        return sorted(content,key=lambda item:(-item.score,item.path))[:100]

    def search_filename(self,query,settings):
        started=time.perf_counter();query=query.strip()
        if not query:return []
        from content import CODE
        excluded={value.lower() if value.startswith('.') else '.'+value.lower() for value in re.split(r'[,;\s]+',settings.get('search_excluded_types','')) if value}
        hidden={normalized(path) for path in settings.get('search_excluded_files',[])}
        categories={'Photos / screenshots':IMAGE,'Documents':{'.pdf','.docx','.xlsx','.pptx'}|TEXT,'Audio / video':AUDIO|VIDEO,'Code':CODE,'Archives / models':{'.zip','.rar','.7z','.gguf','.safetensors','.pt','.pth'}}
        category=settings.get('search_category','All files');results=[];policy=ScanPolicy(settings)
        with self.connect(readonly=True,search=True) as db:
            for path,name,parent,kind in rows_for_search(db,query,settings,categories):
                suffix=Path(path).suffix.lower()
                if not policy.search_allowed(path,settings) or normalized(path) in hidden:continue
                if kind=='folder' and category not in ('All files','Folders'):continue
                if kind!='folder' and (category=='Folders' or suffix in excluded or category in categories and suffix not in categories[category]):continue
                folded=name.casefold();tokens=query.casefold().split()
                exact=folded==query.casefold() or Path(name).stem.casefold()==query.casefold()
                score=2.0 if exact else 1.4 if query.casefold() in folded else .8 if all(token in folded for token in tokens) else .4
                reason='Matches the '+('folder' if kind=='folder' else 'file')+' name' if any(token in folded for token in tokens) else 'Matches words in the folder path'
                result=SearchResult(score,path,'',kind='folder' if kind=='folder' else 'image' if suffix in IMAGE else 'filename',page=1 if kind!='folder' and suffix=='.pdf' else 0,query=query,reasons=[reason])
                result.keyword_score=1;results.append(result)
        results.sort(key=lambda item:(-item.score,len(item.path),item.path))
        logging.getLogger('vectordash').debug('Name search: %.1f ms; %d matches',(time.perf_counter()-started)*1000,len(results))
        return results[:100]

    def search_content(self, query, settings):
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

        cache_ms=(time.perf_counter()-started)*1000
        inference_started=time.perf_counter()
        vector=[]
        if len(query)<2:return []
        if settings['semantic'] and (matrices or metadata.get('_has_vectors')):
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
        inference_ms=(time.perf_counter()-inference_started)*1000
        if not records:
            from disk_search import candidates
            records,matrices,metadata=candidates(self,'code' if code else 'files',query,vector,settings,metadata)
        if not records:return []
        tokens=re.findall(r'[\w]+(?:[-./:][\w]+)*',query.casefold())
        stop={'a','an','the','in','on','at','of','to','and','or','where','they','about','find','shows','show','me','with','is','it'}
        tokens=[token for token in tokens if token not in stop] or tokens
        format_words={'image','images','photo','photos','picture','pictures','video','videos','clip','clips','audio','podcast','podcasts','document','documents','file','files'}
        subjects=[token for token in tokens if token not in format_words]
        if subjects:
            tokens=subjects
        if not tokens:
            return []
        scores=np.asarray(metadata.get('_scores',[0]*len(records)),dtype=np.float32)
        if vector and matrices and len(vector) not in matrices:
            raise ValueError('Embedding dimensions do not match the index; rebuild with the selected model.')
        if vector and len(vector) in matrices:
            indices,matrix=matrices[len(vector)]
            query_vector=np.asarray(vector,dtype=np.float32)
            query_vector/=max(float(np.linalg.norm(query_vector)),1e-30)
            scores[indices]=matrix@query_vector
        excluded={value.lower() if value.startswith('.') else '.'+value.lower() for value in re.split(r'[,;\s]+',settings.get('search_excluded_types','')) if value}
        category=settings.get('search_category','All files')
        intent=set()
        if category=='All files':
            if re.match(r'^(?:images?|photos?|pictures?)\s+(?:of\s+)?',query,re.I): intent={'image'}
            elif any(phrase in query.casefold() for phrase in ('where they talk','podcast','in the recording')): intent={'audio','video_audio'}
            elif re.search(r'\bin (?:a |the )?video\b',query,re.I): intent={'video','video_audio'}
        categories={'Photos / screenshots':{'image'},'Documents':{'text','pdf','document','spreadsheet','slides'},'Audio / video':{'audio','video','video_audio'},'Code':{'code'},'Archives / models':{'archive','model_metadata','filename'}}
        match_names=settings.get('search_names',False)
        text_intent=bool(re.search(r'\d|[_:/-]|\b(?:error|text|screenshot|invoice|code|number)\b',query,re.I))
        eligibility={};policy=ScanPolicy(settings)
        locations=metadata.get('_locations') or [json.loads(record[5]) for record in records]
        image_media={record[1] for i,record in enumerate(records) if record[2]=='image' and locations[i].get('embedding_source')=='media'}
        def eligible(record):
            _,path,kind,_,_,_=record
            cache_key=(path,kind)
            if cache_key in eligibility:return eligibility[cache_key]
            eligibility[cache_key]=not (not policy.search_allowed(path,settings) or intent and kind not in intent or Path(path).suffix.lower() in excluded or category in categories and kind not in categories[category] or os.path.normcase(os.path.abspath(path)) in excluded_files or not safe(Path(Path(path).name)))
            return eligibility[cache_key]
        allowed=[eligible(record) and not (record[2]=='image' and not text_intent and record[1] in image_media and locations[i].get('embedding_source')=='text') for i,record in enumerate(records)]
        modality_means=metadata.get('_modality_means',{})
        if vector and '_modality_means' not in metadata:
            groups={}
            for i,record in enumerate(records):
                if not allowed[i]:
                    continue
                channel=locations[i].get('embedding_source','text')
                group=record[2] if channel=='media' else 'text'
                values=groups.setdefault(group,{})
                values[record[1]]=max(values.get(record[1],-1),float(scores[i]))
            modality_means={group:float(np.mean(list(values.values()))) for group,values in groups.items()}
        token_patterns=[re.compile(r'(?<!\w)'+re.escape(token)+r'(?!\w)') for token in tokens]
        identifier=bool(re.search(r'\d|[_:/-]',query))
        lowered=query.casefold()
        results=[]
        with self.connect(readonly=True,search=True) as db:
            # FTS handles Unicode token boundaries; literal scoring additionally retains punctuation in IDs.
            expression=' OR '.join('"'+token.replace('"','""')+'"' for token in tokens)
            if not match_names:expression='text : ('+expression+')'
            bm25={rowid:-rank for rowid,rank in db.execute('SELECT rowid,bm25(words,1,2) FROM words WHERE words MATCH ? ORDER BY rank LIMIT 400', (expression,))}
            exact=set(bm25)
            bm25_max=max(bm25.values(),default=1)
            # Resolve detailed text/reasons only for a bounded semantic pool plus
            # every keyword hit. Keep channels separate so text cannot crowd out photos.
            candidates={i for i,record in enumerate(records) if allowed[i] and record[0] in exact}
            if vector:
                channels={}
                for i,record in enumerate(records):
                    if allowed[i]:
                        channel=record[2] if locations[i].get('embedding_source')=='media' else 'text'
                        channels.setdefault(channel,[]).append(i)
                for indices in channels.values():
                    indices=np.asarray(indices,dtype=np.intp)
                    limit=min(400,len(indices))
                    chosen=np.argpartition(scores[indices],-limit)[-limit:]
                    candidates.update(int(indices[j]) for j in chosen)
            for index in sorted(candidates):
                rowid,path,kind,text,haystack,location=records[index]
                if not allowed[index]:
                    continue
                haystack=((path+' '+path.replace('_',' ')+' ') if match_names else '')+text.casefold()
                haystack=haystack.casefold()
                matches=sum(bool(pattern.search(haystack)) for pattern in token_patterns)
                literal=query.casefold() in haystack
                lexical=matches/len(tokens)
                semantic=float(scores[index]) if vector else 0
                meta=locations[index]
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
                    if match_names and any(pattern.search(path.casefold().replace('_',' ')) for pattern in token_patterns): reasons.append('Matches words in the filename or folder')
                    if any(pattern.search(text.casefold()) for pattern in token_patterns): reasons.append('Matches text in the picture (OCR)' if kind=='image' else 'Matches words in the content')
                    if vector and semantic>.12: reasons.append({'image':'Matches the picture','video':'Matches this video frame','audio':'Matches this audio moment','video_audio':'Matches audio in the video','pdf':'Matches the appearance of this PDF page'}.get(kind,'Matches the meaning of this passage') if meta.get('embedding_source')=='media' else 'Matches the meaning of this passage')
                    result.reasons=reasons
                    result.lexical_match=bool(matches or literal)
                    result.ranking_channel=group
                    results.append(result)
        if vector and results:
            # Different media have different cosine baselines. Prune within each
            # channel before combining calibrated scores; text must not erase photos.
            best={}
            for result in results:
                best[result.ranking_channel]=max(best.get(result.ranking_channel,-1),result.semantic_score)
            results=[result for result in results if result.lexical_match or result.semantic_score>=max(.3,best[result.ranking_channel]-.13)]
        results.sort(key=lambda item:(-item.score,item.path,item.seconds,item.page))
        selected=[]
        for result in results:
            if any(old.path==result.path and old.kind==result.kind and old.page==result.page and old.line==result.line and (old.seconds<0 and result.seconds<0 or abs(old.seconds-result.seconds)<10) for old in selected):
                continue
            selected.append(result)
            if len(selected)==100:
                break
        logging.getLogger('vectordash').debug('Search completed in %.1f ms: %d results from %d segments; index cache %.1f ms, model/check/wait %.1f ms, ranking %.1f ms',(time.perf_counter()-started)*1000,len(selected),len(records),cache_ms,inference_ms,(time.perf_counter()-started)*1000-cache_ms-inference_ms)
        return selected
