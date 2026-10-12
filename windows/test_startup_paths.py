"""Protected startup scan reuse must respect the saved sensitive-content policy."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from engine import Engine,DEFAULTS

@unittest.skipUnless(os.name=='nt','Windows protected-index startup')
class StartupTests(unittest.TestCase):
    def test_checked_protected_index_avoids_repeat_scan(self):
        with tempfile.TemporaryDirectory() as temp:
            home=Path(temp);engine=Engine(home)
            with engine.connect() as db:
                db.executemany('INSERT OR REPLACE INTO metadata VALUES (?,?)',[('security_version','1'),('content_options',json.dumps({'exclude_sensitive':True}))])
            with patch.object(Engine,'remove_sensitive_legacy_content',autospec=True) as scan:
                Engine(home,DEFAULTS);scan.assert_not_called()

    def test_opted_out_or_old_index_still_scanned(self):
        for options in [{'exclude_sensitive':False},{}]:
            with self.subTest(options=options),tempfile.TemporaryDirectory() as temp:
                home=Path(temp);engine=Engine(home)
                with engine.connect() as db:
                    db.executemany('INSERT OR REPLACE INTO metadata VALUES (?,?)',[('security_version','1'),('content_options',json.dumps(options))])
                with patch.object(Engine,'remove_sensitive_legacy_content',autospec=True) as scan:
                    Engine(home,DEFAULTS);scan.assert_called_once()

class ReindexPlanningTests(unittest.TestCase):
    def test_unchanged_scan_keeps_disk_index_and_metadata_cache(self):
        with tempfile.TemporaryDirectory() as temp:
            home=Path(temp);source=home/'source';source.mkdir();(source/'note.txt').write_text('ordinary document')
            engine=Engine(home);settings=dict(DEFAULTS,roots=[str(source)],semantic=False,ocr_enabled=False)
            engine.index(settings);cache=engine.cached_index('files');version=engine.storage_version;mtime=engine.db.stat().st_mtime_ns
            summary=engine.index(settings)
            self.assertIn('1 reused',summary)
            self.assertEqual(engine.storage_version,version);self.assertEqual(engine.db.stat().st_mtime_ns,mtime)
            self.assertIs(engine.cached_index('files'),cache)
            (source/'note.txt').unlink();engine.index(settings)
            self.assertEqual(engine.indexed_file_count(),0)

    def test_counting_does_not_wait_for_model_or_idle(self):
        with tempfile.TemporaryDirectory() as temp:
            home=Path(temp);source=home/'source';source.mkdir();(source/'note.txt').write_text('ordinary document')
            engine=Engine(home);engine.query_waiting.set()
            settings=dict(DEFAULTS,roots=[str(source)],semantic=False,ocr_enabled=False)
            def progress(text):
                if 'files found' in text:raise InterruptedError('Count ready')
            with self.assertRaisesRegex(InterruptedError,'Count ready'):
                engine.index(settings,progress=progress,condition=lambda:False)

if __name__=='__main__':unittest.main()
