"""Explicit, bounded search memory budgets; database storage always stays on disk."""
from collections import OrderedDict
import threading
import numpy as np

PROFILES={
    'Low memory': {'pages':8,'vectors':0,'delay':150},
    'Balanced': {'pages':32,'vectors':128,'delay':75},
    'Fast search': {'pages':64,'vectors':512,'delay':35},
}

def profile(name):return PROFILES.get(name,PROFILES['Balanced'])

class VectorCache:
    def __init__(self):self.lock=threading.RLock();self.entries=OrderedDict();self.bytes=0
    def clear(self):
        with self.lock:self.entries.clear();self.bytes=0
    def advance(self,old,new):
        with self.lock:self.entries=OrderedDict(((new,*key[1:]),value) for key,value in self.entries.items() if key[0]==old);self.bytes=sum(value['bytes'] for value in self.entries.values())
    def get(self,engine,db,namespace,dimension,mode):
        budget=profile(mode)['vectors']*1024*1024
        if not budget:return None
        key=(engine.storage_version,namespace,dimension,mode)
        with self.lock:
            if key in self.entries:self.entries.move_to_end(key);return self.entries[key]
            count=db.execute('SELECT count(*) FROM segment_vectors WHERE namespace=? AND dimension=?',(namespace,dimension)).fetchone()[0]
            size=count*(dimension*4+16)
            if not count or size>budget:return None
            while self.entries and self.bytes+size>budget:
                _,old=self.entries.popitem(last=False);self.bytes-=old['bytes']
            matrix=np.empty((count,dimension),dtype=np.float32);ids=np.empty(count,dtype=np.int64);groups=np.empty(count,dtype=np.int32)
            metadata=[];mapping={};offset=0
            # Rowid order reads vector pages sequentially. No document text,
            # filename catalog or location JSON is retained in this cache.
            cursor=db.execute('SELECT id,path,kind,channel,vector FROM segment_vectors WHERE namespace=? AND dimension=? ORDER BY id',(namespace,dimension))
            while rows:=cursor.fetchmany(256):
                for rowid,path,kind,channel,blob in rows:
                    item=(path,kind,channel)
                    if item not in mapping:
                        mapping[item]=len(metadata);metadata.append(item);size+=len(path.encode('utf-8'))+len(kind)+len(channel)+160
                        if size>budget:return None
                    ids[offset]=rowid;groups[offset]=mapping[item];matrix[offset]=np.frombuffer(blob,dtype=np.float32);offset+=1
            if offset!=count:return None
            while self.entries and self.bytes+size>budget:
                _,old=self.entries.popitem(last=False);self.bytes-=old['bytes']
            value={'matrix':matrix,'ids':ids,'groups':groups,'metadata':metadata,'bytes':size}
            self.entries[key]=value;self.bytes+=size
            return value
