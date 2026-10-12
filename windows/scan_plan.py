"""Bounded encrypted disk scan staging and compiled source/exclusion rules."""
import os
from pathlib import Path
import tempfile
import secrets
from disk_index import open_database
from types import SimpleNamespace
from vectordash_core import SKIP, SECRET
from security import windows_churn_folders

class ScanPolicy:
    def __init__(self,settings):
        self.roots=sorted([(os.path.normcase(os.path.abspath(root)).rstrip(os.sep),settings.get('source_modes',{}).get(os.path.normcase(os.path.abspath(root)),settings.get('source_modes',{}).get(root,'full'))) for root in settings.get('roots',[])],key=lambda pair:len(pair[0]),reverse=True)
        self.exclusions=[os.path.normcase(os.path.abspath(root)).rstrip(os.sep) for root in settings.get('excluded_folders',[])]
        if settings.get('exclude_windows_churn'):self.exclusions.extend(root.rstrip(os.sep) for root in windows_churn_folders())
        self.by_drive={}
        for root in self.exclusions:self.by_drive.setdefault(os.path.splitdrive(root)[0],[]).append(root)
        import re
        self.suffixes={token.lower() if token.startswith('.') else '.'+token.lower() for token in re.split(r'[,;\s]+',settings.get('index_excluded_types','')) if token}
    def mode(self,path):
        folded=os.path.normcase(str(path))
        for root,mode in self.roots:
            if folded==root or folded.startswith(root+os.sep):return mode
        return 'full'
    def excluded(self,path):
        folded=os.path.normcase(str(path))
        drive=folded[:2] if folded[1:2]==':' else os.path.splitdrive(folded)[0]
        return any(folded==root or folded.startswith(root+os.sep) for root in self.by_drive.get(drive,[])) or bool(self.suffixes and os.path.splitext(folded)[1] in self.suffixes)
    def search_allowed(self,path,settings):
        folded=os.path.normcase(os.path.abspath(path))
        selected=settings.get('search_sources')
        if selected is not None:
            if not hasattr(self,'search_scope'):self.search_scope=[os.path.normcase(os.path.abspath(root)).rstrip(os.sep) for root in selected]
            if not any(folded==root or folded.startswith(root+os.sep) for root in self.search_scope):return False
        return not self.excluded(folded)

    @staticmethod
    def safe_name(name):
        lower=name.lower()
        return not (lower.startswith('.') or lower in SKIP or os.path.splitext(lower)[1] in SECRET or any(word in lower for word in ('password','credential','secret')))

class ScanPlan:
    def __init__(self,home=None,encrypted=True):
        self.count=0
        self.temp=tempfile.TemporaryDirectory(prefix='scan-',dir=home)
        self.path=Path(self.temp.name)/'plan.sqlite'
        self.key=secrets.token_bytes(32) if encrypted else None
        self.db=open_database(self.path,self.key)
        self.db.execute('PRAGMA journal_mode=OFF')
        self.db.executescript("CREATE TABLE catalog(key TEXT PRIMARY KEY,path TEXT,kind TEXT,mtime INTEGER,size INTEGER,mode TEXT); CREATE TABLE previous(key TEXT PRIMARY KEY,path TEXT,kind TEXT,mtime INTEGER,size INTEGER,mode TEXT); CREATE TABLE old_files(path TEXT PRIMARY KEY,mtime INTEGER,size INTEGER); CREATE TABLE old_segments(path TEXT,namespace TEXT,PRIMARY KEY(path,namespace)); CREATE TABLE candidates(key TEXT,namespace TEXT,mode TEXT,changed INTEGER DEFAULT 1,PRIMARY KEY(key,namespace)); CREATE UNIQUE INDEX catalog_path ON catalog(path); CREATE INDEX full_candidates ON candidates(key,namespace) WHERE mode!='names' OR namespace='code';")
    def close(self):self.db.close();self.temp.cleanup()
    def load_previous(self,rows):
        self.db.executemany('INSERT INTO previous VALUES(?,?,?,?,?,?)',((os.path.normcase(path),path,kind,mtime,size,mode) for path,kind,mtime,size,mode in rows))
    def folder(self,path):
        path=str(path);self.db.execute("INSERT OR IGNORE INTO catalog VALUES(?,?,'folder',-1,-1,'full')",(os.path.normcase(path),path))
    def queue(self,path,namespace,stat,mode):
        path=str(path);key=os.path.normcase(path)
        added=self.db.execute('INSERT OR IGNORE INTO candidates(key,namespace,mode) VALUES(?,?,?)',(key,namespace,mode)).rowcount
        if not added:return False
        self.count+=1
        self.db.execute("INSERT INTO catalog VALUES(?,?,'file',?,?,?) ON CONFLICT(key) DO UPDATE SET mode=CASE WHEN excluded.mode='full' THEN 'full' ELSE catalog.mode END",(key,path,stat.st_mtime_ns,stat.st_size,mode))
        return True
    def __len__(self):return self.count
    def entries(self,full_only=False):
        query='SELECT n.path,c.namespace,n.mtime,n.size FROM candidates c JOIN catalog n USING(key)'
        if full_only:query+=" WHERE c.mode!='names' OR c.namespace='code'"
        for path,namespace,mtime,size in self.db.execute(query):yield Path(path),namespace,SimpleNamespace(st_mtime_ns=mtime,st_size=size,st_mode=0,st_file_attributes=0)
    def __iter__(self):return self.entries()
    def old_quick(self,path):
        row=self.db.execute("SELECT mtime,size FROM previous WHERE key=? AND kind='file' AND mode='names'",(os.path.normcase(str(path)),)).fetchone()
        return tuple(row) if row else None
    def mark_quick(self,valid):
        if valid:
            self.db.execute("UPDATE candidates SET changed=0 WHERE mode='names' AND namespace='files' AND EXISTS(SELECT 1 FROM previous p JOIN catalog n USING(key) WHERE p.key=candidates.key AND p.kind='file' AND p.mode='names' AND p.mtime=n.mtime AND p.size=n.size)")
    def mark_legacy_quick(self,path,mtime,size):
        self.db.execute("UPDATE candidates SET changed=0 WHERE key=? AND mode='names' AND namespace='files' AND EXISTS(SELECT 1 FROM catalog n WHERE n.key=candidates.key AND n.mtime=? AND n.size=?)",(os.path.normcase(path),mtime,size))
    def mark(self,path,namespace,changed):self.db.execute('UPDATE candidates SET changed=? WHERE key=? AND namespace=?',(int(changed),os.path.normcase(str(path)),namespace))
    def planned(self,path,namespace):return bool(self.db.execute('SELECT changed FROM candidates WHERE key=? AND namespace=?',(os.path.normcase(str(path)),namespace)).fetchone()[0])
    def needed(self):return self.db.execute('SELECT coalesce(sum(changed),0) FROM candidates').fetchone()[0]
    def full_needed(self):return self.db.execute("SELECT count(*) FROM candidates WHERE changed=1 AND (mode!='names' OR namespace='code')").fetchone()[0]
    def quick_counts(self):
        changed,total=self.db.execute("SELECT coalesce(sum(changed),0),count(*) FROM candidates WHERE mode='names' AND namespace='files'").fetchone()
        return changed,total-changed
    def names_changed(self):
        return bool(self.db.execute("SELECT 1 FROM catalog n LEFT JOIN previous p USING(key) WHERE p.key IS NULL OR n.path!=p.path OR n.kind!=p.kind OR n.mtime!=p.mtime OR n.size!=p.size OR n.mode!=p.mode LIMIT 1").fetchone() or self.db.execute('SELECT 1 FROM previous p LEFT JOIN catalog n USING(key) WHERE n.key IS NULL LIMIT 1').fetchone())
    def removals(self):return (row[0] for row in self.db.execute('SELECT p.path FROM previous p LEFT JOIN catalog n USING(key) WHERE n.key IS NULL'))
    def changes(self):
        for path,kind,mtime,size,mode,old_path in self.db.execute("SELECT n.path,n.kind,n.mtime,n.size,n.mode,p.path FROM catalog n LEFT JOIN previous p USING(key) WHERE p.key IS NULL OR n.path!=p.path OR n.kind!=p.kind OR n.mtime!=p.mtime OR n.size!=p.size OR n.mode!=p.mode"):
            file=Path(path)
            yield (path,file.name or path,str(file.parent),kind,path.casefold(),mtime,size,mode),old_path

    def load_files(self,rows):self.db.executemany('INSERT INTO old_files VALUES(?,?,?)',rows)
    def load_segments(self,rows):self.db.executemany('INSERT OR IGNORE INTO old_segments VALUES(?,?)',rows)
    def old_file(self,path):
        row=self.db.execute('SELECT mtime,size FROM old_files WHERE path=?',(str(path),)).fetchone()
        return (None,None,*row) if row else None
    def has_segment(self,path,namespace):return bool(self.db.execute('SELECT 1 FROM old_segments WHERE path=? AND namespace=?',(str(path),namespace)).fetchone())
    def files(self):return ((path,(None,None,mtime,size)) for path,mtime,size in self.db.execute('SELECT * FROM old_files'))
    def full_keys_changed(self):
        return bool(self.db.execute("SELECT 1 FROM candidates c JOIN catalog n USING(key) LEFT JOIN old_segments o ON o.path=n.path AND o.namespace=c.namespace WHERE (c.mode!='names' OR c.namespace='code') AND o.path IS NULL LIMIT 1").fetchone() or self.db.execute("SELECT 1 FROM old_segments o WHERE NOT EXISTS(SELECT 1 FROM candidates c JOIN catalog n USING(key) WHERE n.path=o.path AND c.namespace=o.namespace AND (c.mode!='names' OR c.namespace='code')) LIMIT 1").fetchone())
    def full_count(self):return self.db.execute("SELECT count(*) FROM candidates WHERE mode!='names' OR namespace='code'").fetchone()[0]

    def source_counts(self,roots):
        result={}
        for root in roots:
            key=os.path.normcase(os.path.abspath(root));prefix=key.rstrip(os.sep)+os.sep
            upper=prefix[:-1]+chr(ord(prefix[-1])+1)
            row=self.db.execute("SELECT count(*),coalesce(sum(p.key IS NULL),0) FROM catalog n LEFT JOIN previous p USING(key) WHERE n.kind='file' AND (n.key=? OR (n.key>=? AND n.key<?))",(key,prefix,upper)).fetchone()
            result[key]={'files':row[0],'new':row[1]}
        return result
