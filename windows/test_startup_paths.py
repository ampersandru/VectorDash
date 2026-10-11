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

if __name__=='__main__':unittest.main()
