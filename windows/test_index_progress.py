"""Incremental planning, durable cancellation resume and protected storage regressions."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import os
from engine import Engine,DEFAULTS
from content import Segment
from security import seal,unseal,MAGIC

class IndexProgressTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.home=Path(self.temp.name);self.source=self.home/'source';self.source.mkdir()
        self.engine=Engine(self.home)
        self.settings=dict(DEFAULTS,roots=[str(self.source)],ocr_enabled=False)
        for name in ('first.txt','second.txt'):(self.source/name).write_text('ordinary searchable content '+name)

    def test_counts_incremental_changed_and_removed_files(self):
        states=[];self.engine.index(self.settings,file_progress=states.append)
        self.assertEqual(states[-1]['completed'],2);self.assertEqual(states[-1]['remaining'],0)
        with patch('engine.extract',side_effect=AssertionError('unchanged file parsed')):
            states=[];result=self.engine.index(self.settings,file_progress=states.append)
        self.assertIn('2 reused',result);self.assertEqual(states[-1]['needs_index'],0)
        (self.source/'first.txt').write_text('changed content with a different length')
        states=[];self.engine.index(self.settings,file_progress=states.append)
        self.assertEqual(states[-1]['needs_index'],1);self.assertEqual(states[-1]['reused'],1)
        (self.source/'second.txt').unlink()
        self.engine.index(self.settings);self.assertEqual(self.engine.indexed_file_count(),1)

    def test_cancel_keeps_old_index_and_resumes_completed_file_after_restart(self):
        self.engine.index(self.settings)
        for name in ('first.txt','second.txt'):(self.source/name).write_text('new content changed '+name)
        def stop_after_first(state):
            if state['completed']==1:self.engine.cancel.set()
        with self.assertRaises(InterruptedError):self.engine.index(self.settings,file_progress=stop_after_first)
        self.assertTrue(self.engine.search('ordinary',self.settings))
        cache=list((self.home/'index-resume').glob('*.cache'));self.assertEqual(len(cache),1)
        if os.name=='nt':
            data=cache[0].read_bytes();self.assertTrue(data.startswith(MAGIC));self.assertNotIn(b'new content',data)
        restarted=Engine(self.home);states=[]
        with patch('engine.extract',wraps=__import__('engine').extract) as extract:
            result=restarted.index(self.settings,file_progress=states.append)
            self.assertEqual(extract.call_count,1)
        self.assertIn('1 resumed',result);self.assertEqual(states[-1]['remaining'],0)
        self.assertFalse(list((self.home/'index-resume').glob('*.cache')))
        self.assertFalse(restarted.search('ordinary',self.settings))

    def test_configuration_change_invalidates_resume(self):
        def stop(state):
            if state['completed']==1:self.engine.cancel.set()
        with self.assertRaises(InterruptedError):self.engine.index(self.settings,file_progress=stop)
        self.engine.cancel.clear()
        with patch('engine.extract',wraps=__import__('engine').extract) as extract:
            self.engine.index(dict(self.settings,pdf_visuals=True))
            self.assertEqual(extract.call_count,2)

    @unittest.skipUnless(os.name=='nt','Windows DPAPI')
    def test_large_protected_storage_roundtrip_and_tamper(self):
        data=b'private index content '*1000
        with patch('security.CHUNK_SIZE',4096):
            encrypted=seal(data);self.assertEqual(unseal(encrypted),data)
            self.assertNotIn(b'private index',encrypted)
            with self.assertRaises((OSError,ValueError)):unseal(encrypted[:-1])
        # Old single-block files continue to open.
        self.assertEqual(unseal(seal(b'legacy small payload')),b'legacy small payload')

if __name__=='__main__':unittest.main()
