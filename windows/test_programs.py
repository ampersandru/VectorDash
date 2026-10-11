import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from programs import ProgramCatalog, SOURCE
from engine import Engine, DEFAULTS

class ProgramsTest(unittest.TestCase):
    def test_scan_roundtrip_duplicates_and_store_apps(self):
        with tempfile.TemporaryDirectory() as temp:
            home=Path(temp);user=home/'user';common=home/'common'
            for root in (user,common):
                (root/'Tools').mkdir(parents=True)
                (root/'Tools/Editor.lnk').write_bytes(b'not parsed or executed')
            (common/'Game.url').write_text('[InternetShortcut]')
            (common/'Ignore.exe').write_bytes(b'ignore')
            catalog=ProgramCatalog(home)
            count,warnings=catalog.scan([user,common],[{'Name':'Calculator','AppID':'Microsoft.Calculator_test!App'},{'Name':'Invalid','AppID':'bad;command!App'}])
            self.assertEqual(count,3);self.assertFalse(warnings)
            enabled=dict(DEFAULTS,programs_enabled=True)
            result=catalog.search('editor',enabled)[0]
            self.assertEqual(result.path,str(user/'Tools/Editor.lnk'))
            self.assertEqual(result.symbol,'Editor');self.assertEqual(result.kind,'application')
            self.assertEqual(len(ProgramCatalog(home).entries),3)
            if os.name=='nt':self.assertNotIn(b'Calculator',catalog.path.read_bytes())
            self.assertFalse(catalog.search('editor',dict(enabled,programs_enabled=False)))
            self.assertFalse(catalog.search('editor',dict(enabled,search_sources=[])))
            self.assertTrue(catalog.search('editor',dict(enabled,search_sources=[SOURCE])))
            self.assertFalse(catalog.search('editor',dict(enabled,search_category='Documents')))
            self.assertTrue(catalog.search('app: editor',dict(enabled,search_category='Documents')))
            hidden=os.path.normcase(os.path.abspath(result.path))
            self.assertFalse(catalog.search('editor',dict(enabled,search_excluded_files=[hidden])))

    def test_app_search_never_requests_embeddings_or_file_index(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/'shortcuts';root.mkdir();(root/'Editor.lnk').write_bytes(b'')
            engine=Engine(temp,DEFAULTS);engine.programs.scan([root],[])
            with patch.object(engine,'search_files',side_effect=AssertionError('file search called')):
                self.assertEqual(engine.search('app: editor',dict(DEFAULTS,programs_enabled=True))[0].symbol,'Editor')
                self.assertEqual(engine.search('editor',dict(DEFAULTS,programs_enabled=True,search_category='Programs'))[0].symbol,'Editor')

    def test_file_failure_keeps_program_results(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/'shortcuts';root.mkdir();(root/'Editor.lnk').write_bytes(b'')
            engine=Engine(temp,DEFAULTS);engine.programs.scan([root],[])
            with patch.object(engine,'search_files',side_effect=RuntimeError('offline')), self.assertLogs('vectordash',level='WARNING'):
                self.assertEqual(engine.search('editor',dict(DEFAULTS,programs_enabled=True))[0].kind,'application')
                with self.assertRaises(RuntimeError):engine.search('unknown',dict(DEFAULTS,programs_enabled=True))

class ProgramUiTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from PySide6.QtWidgets import QApplication
        cls.app=QApplication.instance() or QApplication([])

    def test_program_preview_does_not_read_shortcut_and_launch_preserves_path(self):
        from types import SimpleNamespace
        from unittest.mock import Mock
        from desktop import Desktop
        from engine import SearchResult
        from previews import Preview
        result=SearchResult(3.5,'shell:AppsFolder\\Microsoft.Calculator_test!App','Windows app',kind='application',symbol='Calculator')
        with patch.object(Preview,'text',side_effect=AssertionError('shortcut read as text')):
            preview=Preview(result,embedded=True)
            self.assertIn('Program',preview.caption.text())
            preview.close()
        owner=SimpleNamespace(quick=Mock(),error=Mock())
        with patch('desktop.os.startfile',create=True) as launch:
            Desktop.open_file(owner,result)
            launch.assert_called_once_with(result.path)
            owner.quick.hide.assert_called_once();owner.error.assert_not_called()

    def test_explicit_program_search_does_not_queue_behind_file_search(self):
        from types import SimpleNamespace
        from unittest.mock import Mock
        from desktop import Desktop
        owner=SimpleNamespace(search_sequence=5,settings={'search_category':'Programs'},engine=Mock(),results=Mock(),quick=Mock(),search_hint=Mock(),search_running=True,pending_search='old')
        owner.engine.programs.search.return_value=[]
        Desktop.request_search(owner,'calculator')
        self.assertEqual(owner.search_sequence,6);self.assertIsNone(owner.pending_search)
        owner.results.populate.assert_called_once_with([])
        owner.quick.hint.setText.assert_called_once_with('0 programs')

if __name__=='__main__':unittest.main()
