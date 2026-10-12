"""Disk storage, upgrade, encryption, and bounded retrieval regressions."""
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import threading
import unittest
from unittest.mock import patch
import numpy as np
from engine import Engine,DEFAULTS
from disk_index import LegacyReader
from security import seal_to_file, CHUNK_SIZE
from scan_plan import ScanPlan

class DiskStorageTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.home=Path(self.temp.name)

    @unittest.skipUnless(os.name=='nt','DPAPI buffers')
    def test_dpapi_inputs_are_released_without_cyclic_gc(self):
        import subprocess,sys
        script="""
import gc,psutil
from security import crypt
gc.collect();baseline=psutil.Process().memory_info().rss;gc.disable()
for i in range(40):
    data=crypt(b'x'*1048576)
    assert crypt(data,True)==b'x'*1048576
assert psutil.Process().memory_info().rss-baseline<16*1024*1024
"""
        result=subprocess.run([sys.executable,'-c',script],cwd=Path(__file__).parent,capture_output=True,text=True,timeout=15)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)

    def test_disk_pages_and_scan_staging_have_bounded_caches(self):
        engine=Engine(self.home)
        with engine.connect(readonly=True) as db:
            self.assertEqual(Path(db.execute('PRAGMA database_list').fetchone()[2]),engine.db)
            self.assertEqual(db.execute('PRAGMA cache_size').fetchone()[0],-8192)
            self.assertEqual(db.execute('PRAGMA mmap_size').fetchone()[0],0)
        plan=ScanPlan(self.home)
        try:
            plan.folder(self.home/'PRIVATE_SCAN_PATH');plan.db.commit()
            self.assertTrue(plan.path.exists())
            self.assertNotIn(b'PRIVATE_SCAN_PATH',plan.path.read_bytes())
        finally:plan.close()
        self.assertFalse(plan.path.exists())

    @unittest.skipUnless(os.name=='nt','Windows account encryption')
    def test_encryption_toggle_preserves_content_names_and_vectors(self):
        engine=Engine(self.home)
        path=str(self.home/'PRIVATE_DOCUMENT.txt')
        with engine.connect() as db:
            db.execute("INSERT INTO segments VALUES(?,'files','text','PRIVATE_SENTINEL invoice','[1,0]','{}')",(path,))
            db.execute('INSERT INTO words(rowid,path,text) SELECT rowid,path,text FROM segments')
        for encrypted in (False,True):
            reopened=Engine(self.home,dict(DEFAULTS,encrypt_index=encrypted,exclude_sensitive=False))
            stored=reopened.db.read_bytes()
            self.assertEqual(stored.startswith(b'SQLite format 3'),not encrypted)
            self.assertEqual(b'PRIVATE_SENTINEL' in stored,not encrypted)
            self.assertTrue(reopened.search('PRIVATE_SENTINEL',dict(DEFAULTS,semantic=False)))
            with reopened.connect(readonly=True) as db:self.assertEqual(db.execute('SELECT dimension FROM segment_vectors').fetchone()[0],2)

    @unittest.skipUnless(os.name=='nt','Windows protected-index upgrade')
    def test_chunked_legacy_upgrade_streams_and_preserves_fts(self):
        db=sqlite3.connect(':memory:')
        db.executescript("CREATE TABLE files(path TEXT PRIMARY KEY,text TEXT,vector TEXT); CREATE TABLE segments(path TEXT,namespace TEXT,kind TEXT,text TEXT,vector TEXT,location TEXT); CREATE VIRTUAL TABLE words USING fts5(path,text); CREATE TABLE filler(data BLOB);")
        db.execute("INSERT INTO filler VALUES(zeroblob(?))",(CHUNK_SIZE+100,))
        db.execute("INSERT INTO files VALUES('legacy.txt','invoice INV-779','[]')")
        db.execute("INSERT INTO segments VALUES('legacy.txt','files','text','invoice INV-779','[1,0]','{}')")
        db.execute('INSERT INTO words(rowid,path,text) SELECT rowid,path,text FROM segments');db.commit()
        seal_to_file(db.serialize(),self.home/'index.sqlite');db.close()
        reader=LegacyReader(self.home/'index.sqlite')
        try:
            for offset in (0,CHUNK_SIZE,4096):reader.read(100,offset)
            self.assertLessEqual(len(reader.cache),2)
        finally:reader.close()
        # Both former whole-database APIs are forbidden during conversion.
        with patch('security.unseal_file',side_effect=AssertionError('whole DB decrypted')),patch('security.seal_to_file',side_effect=AssertionError('whole DB serialized')):
            engine=Engine(self.home,dict(DEFAULTS,exclude_sensitive=False))
        self.assertTrue(engine.search('INV-779',dict(DEFAULTS,semantic=False)))
        self.assertFalse(self.home.joinpath('index.legacy.dpapi').exists())

    def test_conversion_is_background_and_readiness_is_explicit(self):
        db=sqlite3.connect(self.home/'index.sqlite');db.execute('CREATE TABLE files(path TEXT PRIMARY KEY,text TEXT,vector TEXT)');db.commit();db.close()
        import engine as module
        actual=module.change_encryption;entered=threading.Event();release=threading.Event()
        def slow(*args,**kwargs):entered.set();release.wait(3);return actual(*args,**kwargs)
        with patch('engine.change_encryption',side_effect=slow):
            engine=Engine(self.home,DEFAULTS,defer=True)
            self.assertTrue(entered.wait(1));self.assertFalse(engine.ready.is_set())
            self.assertEqual(engine.index_stats()['total'],0)
            queried=threading.Event()
            def query():engine.cached_index('files');queried.set()
            search=threading.Thread(target=query,daemon=True);search.start()
            release.set();self.assertTrue(engine.ready.wait(3));engine.wait_ready()
            self.assertTrue(queried.wait(3));search.join(1)

    def test_semantic_candidates_and_reusable_cache_are_bounded(self):
        engine=Engine(self.home)
        with engine.connect() as db:
            for i in range(1200):
                vector=[1,i/1200]
                db.execute('INSERT INTO segments VALUES(?,?,?,?,?,?)',(f'file-{i}.txt','files','text','generic text',json.dumps(vector),'{}'))
            db.execute('INSERT INTO words(rowid,path,text) SELECT rowid,path,text FROM segments')
        from disk_search import candidates,POOL
        records,matrices,metadata=candidates(engine,'files','subject',[1,0],DEFAULTS,{})
        self.assertLessEqual(len(records),POOL);self.assertFalse(matrices)
        self.assertEqual(len(metadata['_scores']),len(records))
        cached=engine.cached_index('files');self.assertEqual(cached[:2],([],{}))
        self.assertTrue(cached[2]['_has_vectors'])

if __name__=='__main__':unittest.main()
