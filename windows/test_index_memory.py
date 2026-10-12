import os
from pathlib import Path
import tempfile
import threading
import time
import unittest
from unittest.mock import patch
from engine import Engine,DEFAULTS
from name_index import normalized
from security import CHUNK_SIZE,seal,unseal,seal_to_file,unseal_file

class IndexMemoryTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.home=Path(self.temp.name);self.root=self.home/'source';self.root.mkdir();self.data=self.home/'data';self.data.mkdir()
        self.engine=Engine(self.data)
        self.settings=dict(DEFAULTS,roots=[str(self.root)],source_modes={normalized(self.root):'names'},semantic=False,ocr_enabled=False)

    def test_unchanged_update_never_opens_writer_and_reopen_skips_schema_copy(self):
        (self.root/'stable.txt').write_text('stable')
        self.engine.index(self.settings)
        connect=self.engine.connect;requests=[]
        def inspect(*args,**kwargs):requests.append(kwargs.get('readonly',False));return connect(*args,**kwargs)
        with patch.object(self.engine,'connect',side_effect=inspect):self.engine.index(self.settings)
        self.assertTrue(requests);self.assertTrue(all(requests))
        with patch.object(Engine,'initialize_schema',side_effect=AssertionError('unnecessary schema writer')):
            reopened=Engine(self.data,self.settings)
            with reopened.connect(readonly=True):pass
            self.assertFalse(hasattr(reopened,'snapshot'))

    def test_quick_change_retains_content_cache_and_catalog_rowids(self):
        changed=self.root/'change.txt';changed.write_text('original')
        stable=self.root/'stable.txt';stable.write_text('stable')
        self.engine.index(self.settings);cached=self.engine.cached_index('files')
        with self.engine.connect(readonly=True) as db:rowid=db.execute('SELECT rowid FROM names WHERE path=?',(str(stable),)).fetchone()[0]
        changed.write_text('changed and larger')
        self.engine.index(self.settings)
        self.assertIs(self.engine.cached_index('files'),cached)
        with self.engine.connect(readonly=True) as db:self.assertEqual(db.execute('SELECT rowid FROM names WHERE path=?',(str(stable),)).fetchone()[0],rowid)
        self.assertEqual(self.engine.index_stats()['quick'],2)

    def test_overlapping_roots_scanned_once_and_specific_full_mode_wins(self):
        nested=self.root/'documents';nested.mkdir();(nested/'note.txt').write_text('specific content')
        (self.root/'quick.txt').write_text('names only')
        settings=dict(self.settings,roots=[str(nested),str(self.root)],source_modes={normalized(self.root):'names',normalized(nested):'full'})
        scandir=os.scandir;calls=[]
        def track(path):calls.append(normalized(path));return scandir(path)
        with patch('engine.os.scandir',side_effect=track):self.engine.index(settings)
        self.assertEqual(calls.count(normalized(nested)),1)
        self.assertTrue(self.engine.search('specific',dict(settings,search_mode='Semantic')))
        self.assertEqual(self.engine.index_stats()['quick'],1)

    def test_explicit_source_inside_pruned_parent_is_still_scanned(self):
        hidden=self.root/'.cache';hidden.mkdir();nested=hidden/'personal';nested.mkdir()
        note=nested/'note.txt';note.write_text('explicit personal content')
        settings=dict(self.settings,roots=[str(self.root),str(nested)],source_modes={normalized(self.root):'names',normalized(nested):'full'})
        self.engine.index(settings)
        self.assertTrue(any(result.path==str(note) for result in self.engine.search('explicit personal content',dict(settings,search_mode='Semantic'))))

    def test_churn_option_removes_cache_without_reparsing_personal_content(self):
        churn=self.root/'churn';churn.mkdir();(churn/'noise.txt').write_text('frequent cache')
        (self.root/'personal.txt').write_text('personal content')
        settings=dict(self.settings,source_modes={normalized(self.root):'full'})
        self.engine.index(settings)
        exclusions=[normalized(churn)]
        with patch('scan_plan.windows_churn_folders',return_value=exclusions),patch('security.windows_churn_folders',return_value=exclusions),patch('engine.isolated_extract',side_effect=AssertionError('retained content reparsed')):
            self.engine.index(dict(settings,exclude_windows_churn=True))
        self.assertEqual(self.engine.index_stats()['full'],1)
        self.assertFalse(self.engine.search('noise',dict(settings,search_mode='Names')))

    @unittest.skipUnless(os.name=='nt','DPAPI encryption')
    def test_streaming_protection_is_compatible_and_detects_truncation(self):
        payload=b'x'*(CHUNK_SIZE+100);path=self.home/'protected'
        seal_to_file(payload,path);self.assertEqual(unseal_file(path),payload);self.assertEqual(unseal(path.read_bytes()),payload)
        path.write_bytes(seal(payload));self.assertEqual(unseal_file(path),payload)
        path.write_bytes(path.read_bytes()[:-10])
        with self.assertRaises(ValueError):unseal_file(path)

    def test_search_not_blocked_by_disk_writer(self):
        (self.root/'stable.txt').write_text('stable');self.engine.index(self.settings)
        entered=threading.Event();release=threading.Event();errors=[]
        def write():
            try:
                with self.engine.connect() as db:
                    db.execute("INSERT OR REPLACE INTO metadata VALUES('probe','value')")
                    entered.set();release.wait(3)
            except Exception as error:errors.append(error)
        worker=threading.Thread(target=write);worker.start();self.assertTrue(entered.wait(2))
        started=time.perf_counter()
        try:self.assertTrue(self.engine.search('stable',dict(self.settings,search_mode='Names')))
        finally:release.set();worker.join(4)
        self.assertLess(time.perf_counter()-started,1);self.assertFalse(errors)

    def test_cancel_before_disk_commit_keeps_previous_index(self):
        (self.root/'original.txt').write_text('original');self.engine.index(self.settings)
        version=self.engine.storage_version;(self.root/'new.txt').write_text('new')
        from name_index import sync_catalog
        def cancel_save(db,changes,removals):
            sync_catalog(db,changes,removals);self.engine.cancel.set()
        with patch('engine.sync_catalog',side_effect=cancel_save):
            with self.assertRaises(InterruptedError):self.engine.index(self.settings)
        self.engine.cancel.clear();self.assertEqual(self.engine.storage_version,version)
        self.assertFalse(self.engine.search('new.txt',dict(self.settings,search_mode='Names')))

    def test_unchanged_failed_file_is_not_reparsed_each_update(self):
        (self.root/'broken.pdf').write_text('unchanged file')
        settings=dict(self.settings,source_modes={normalized(self.root):'full'})
        with patch('engine.isolated_extract',side_effect=ValueError('bad parser input')) as extract:
            self.engine.index(settings);self.engine.index(settings)
            self.assertEqual(extract.call_count,1)

if __name__=='__main__':unittest.main()
