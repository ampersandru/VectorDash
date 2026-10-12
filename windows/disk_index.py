"""Encrypted, disk-backed SQLite with a small page cache and streaming upgrades."""
from pathlib import Path
import os
import secrets
import struct
import hashlib
from collections import OrderedDict
from contextlib import contextmanager
from sqlcipher3 import dbapi2 as sqlite3
from security import MAGIC, CHUNK_MARKER, CHUNK_SIZE, crypt, seal, unseal

READ_CACHE_KIB=8192
WRITE_CACHE_KIB=16384

def open_database(path,key,readonly=False,cache_kib=None):
    # A raw random key avoids an expensive password KDF on every connection.
    db=sqlite3.connect(str(path),timeout=5,check_same_thread=False)
    if key is not None:db.execute('PRAGMA key="x\''+key.hex()+'\'"')
    db.execute('PRAGMA cache_size=-'+str(cache_kib if cache_kib is not None else (READ_CACHE_KIB if readonly else WRITE_CACHE_KIB)))
    db.execute('PRAGMA mmap_size=0')
    # SQLCipher is compiled with memory-only SQLite scratch storage. Large
    # staging tables use separate encrypted databases, never TEMP tables.
    db.execute('PRAGMA temp_store=MEMORY')
    db.execute('PRAGMA synchronous=NORMAL')
    if readonly:db.execute('PRAGMA query_only=ON')
    return db

def account_key(home):
    path=Path(home)/'index.key'
    if path.exists():
        key=unseal(path.read_bytes())
        if len(key)!=32:raise ValueError('Invalid index encryption key')
        return key
    key=secrets.token_bytes(32)
    temporary=path.with_suffix('.key.tmp');temporary.write_bytes(seal(key));temporary.replace(path)
    return key

class LegacyReader:
    """Random access to old DPAPI chunks with at most two chunks cached."""
    def __init__(self,path):
        self.stream=open(path,'rb');self.chunks=[];self.cache=OrderedDict()
        self.stream.seek(len(MAGIC))
        if self.stream.read(len(CHUNK_MARKER))!=CHUNK_MARKER:
            if os.path.getsize(path)>CHUNK_SIZE+1024*1024:
                self.stream.close();raise ValueError('This old single-block index must be rebuilt to avoid loading it into memory.')
            self.stream.seek(len(MAGIC));self.single=crypt(self.stream.read(),True)
            # The pre-chunk format was used only for indexes <=8 MiB.
            self.size=len(self.single);return
        self.single=None;raw=self.stream.read(4)
        if len(raw)!=4:raise ValueError('Truncated protected index')
        count=struct.unpack('<I',raw)[0];hasher=hashlib.sha256();digest=None;total=0
        if count<2 or count>os.path.getsize(path)//4:raise ValueError('Invalid protected chunk count')
        for number in range(count):
            raw=self.stream.read(4)
            if len(raw)!=4:raise ValueError('Truncated protected index')
            size=struct.unpack('<I',raw)[0];offset=self.stream.tell()
            if size>CHUNK_SIZE+1024*1024:raise ValueError('Invalid protected chunk size')
            payload=self.stream.read(size)
            if len(payload)!=size:raise ValueError('Truncated protected index')
            chunk=crypt(payload,True)
            if len(chunk)<40 or struct.unpack_from('<II',chunk)!=(number,count):raise ValueError('Invalid protected chunk order')
            if digest is None:digest=chunk[8:40]
            if chunk[8:40]!=digest:raise ValueError('Mixed protected chunks')
            length=len(chunk)-40
            if number<count-1 and length!=CHUNK_SIZE:raise ValueError('Invalid protected chunk length')
            hasher.update(memoryview(chunk)[40:]);self.chunks.append((offset,size,length));total+=length
        if self.stream.read(1) or hasher.digest()!=digest:raise ValueError('Protected index integrity mismatch')
        self.size=total
    def read(self,amount,offset):
        start=offset
        if self.single is not None:result=self.single[offset:offset+amount]
        else:
            output=bytearray()
            while amount and offset<self.size:
                number,within=divmod(offset,CHUNK_SIZE)
                chunk=self.cache.get(number)
                if chunk is None:
                    position,size,_=self.chunks[number];self.stream.seek(position)
                    chunk=crypt(self.stream.read(size),True)[40:];self.cache[number]=chunk
                    if len(self.cache)>2:self.cache.popitem(last=False)
                self.cache.move_to_end(number)
                take=min(amount,len(chunk)-within);output.extend(chunk[within:within+take]);offset+=take;amount-=take
            result=bytes(output)
        # serialize() could retain WAL header flags; this immutable source has
        # no separate WAL. It must be read as a self-contained rollback DB.
        result=bytearray(result)
        for position in (18,19):
            if start<=position<start+len(result):result[position-start]=1
        return bytes(result)
    def close(self):self.cache.clear();self.stream.close()

@contextmanager
def legacy_database(path):
    import apsw
    reader=LegacyReader(path)
    name='vectordash-legacy-'+secrets.token_hex(8)
    class File(apsw.VFSFile):
        def xRead(self,amount,offset):
            data=reader.read(amount,offset)
            return data+b'\0'*(amount-len(data))
        def xFileSize(self):return reader.size
    class VFS(apsw.VFS):
        def xOpen(self,filename,flags):return File('',filename,flags)
    vfs=VFS(name,'');db=None
    try:
        db=apsw.Connection(Path(path).resolve().as_uri()+'?immutable=1',flags=apsw.SQLITE_OPEN_READONLY|apsw.SQLITE_OPEN_URI,vfs=name)
        db.execute('PRAGMA cache_size=-8192');db.execute('PRAGMA mmap_size=0')
        yield db
    finally:
        if db:db.close()
        vfs.unregister();reader.close()

def migrate(path,key,progress=lambda text:None):
    """Copy old SQLite rows directly to SQLCipher, never plaintext disk/RAM DB."""
    import time
    path=Path(path);temporary=path.with_suffix('.converting')
    for suffix in ('','-journal','-wal','-shm'):Path(str(temporary)+suffix).unlink(missing_ok=True)
    with path.open('rb') as stream:protected=stream.read(len(MAGIC))==MAGIC
    if protected:source_context=legacy_database(path)
    else:
        @contextmanager
        def plain():
            import sqlite3 as standard
            db=standard.connect(str(path));db.execute('PRAGMA cache_size=-8192')
            try:yield db
            finally:db.close()
        source_context=plain()
    target=open_database(temporary,key)
    try:
        with source_context as source:
            definitions=list(source.execute("SELECT type,name,sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY rowid"))
            for kind,name,sql in definitions:
                if kind=='table' and not target.execute('SELECT 1 FROM sqlite_master WHERE name=?',(name,)).fetchone():target.execute(sql)
            target.commit()
            for kind,name,sql in definitions:
                if kind!='table' or sql.lstrip().upper().startswith('CREATE VIRTUAL'):continue
                quoted='"'+name.replace('"','""')+'"'
                columns=[row[1] for row in source.execute('PRAGMA table_info('+quoted+')')]
                if 'WITHOUT ROWID' not in sql.upper() and 'rowid' not in columns:columns=['rowid']+columns
                fields=','.join('"'+column.replace('"','""')+'"' for column in columns)
                target.execute('DELETE FROM '+quoted)
                cursor=iter(source.execute('SELECT '+fields+' FROM '+quoted));copied=0
                import itertools
                while batch:=list(itertools.islice(cursor,1000)):
                    target.executemany('INSERT INTO '+quoted+'('+fields+') VALUES('+','.join('?' for _ in columns)+')',batch)
                    copied+=len(batch)
                    if copied%10000==0:progress('Preparing stored index: '+name+' · '+f'{copied:,}'+' entries');time.sleep(.001)
                target.commit()
            for kind,name,sql in definitions:
                if kind!='table':target.execute(sql)
            target.commit()
            if target.execute('PRAGMA quick_check').fetchone()[0]!='ok':raise ValueError('Converted index failed validation')
        target.close();target=None
        backup=path.with_suffix('.legacy.dpapi' if protected else '.legacy.sqlite')
        path.replace(backup)
        try:temporary.replace(path)
        except BaseException:backup.replace(path);raise
        for suffix in ('-wal','-shm'):Path(str(path)+suffix).unlink(missing_ok=True)
        # Validated replacement is durable; keep no duplicate multi-GB index.
        backup.unlink()
    finally:
        if target:target.close()
        temporary.unlink(missing_ok=True)


def change_encryption(path,old_key,new_key,progress=lambda text:None):
    """SQLCipher exports page-by-page, including FTS, using bounded caches."""
    path=Path(path);temporary=path.with_suffix('.converting')
    for suffix in ('','-journal','-wal','-shm'):Path(str(temporary)+suffix).unlink(missing_ok=True)
    source=open_database(path,old_key)
    try:
        literal="x'"+new_key.hex()+"'" if new_key is not None else ''
        source.execute('ATTACH DATABASE ? AS converted KEY ?',(str(temporary),literal))
        source.execute('PRAGMA converted.cache_size=-8192')
        progress('Updating database encryption…')
        source.execute("SELECT sqlcipher_export('converted')")
        source.execute('DETACH DATABASE converted')
        source.close();source=None
        check=open_database(temporary,new_key)
        try:
            if check.execute('PRAGMA quick_check').fetchone()[0]!='ok':raise ValueError('Converted database failed validation')
        finally:check.close()
        # Checkpoint the old connection before replacing its main file.
        backup=path.with_suffix('.encryption-backup');path.replace(backup)
        try:temporary.replace(path)
        except BaseException:backup.replace(path);raise
        for suffix in ('-wal','-shm'):Path(str(path)+suffix).unlink(missing_ok=True)
        backup.unlink()
    finally:
        if source:source.close()
        temporary.unlink(missing_ok=True)
