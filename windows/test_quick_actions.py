import unittest
from types import SimpleNamespace
from unittest.mock import Mock,patch
from quick_actions import calculate,results,SETTING_URIS
from engine import DEFAULTS,Engine
from desktop import Desktop
from previews import Preview
from PySide6.QtWidgets import QApplication

class QuickActionsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.app=QApplication.instance() or QApplication([])

    def test_calculator_arithmetic_percent_functions_and_bounds(self):
        for expression,answer in [('2+3*4','14'),('(2+3)*4','20'),('2^8','256'),('20% of 150','30'),('15%','0.15'),('10%3','1'),('sqrt(81)','9'),('round(pi,2)','3.14'),('0.1+0.2','0.3'),('-5 + 2','-3')]:
            self.assertEqual(calculate(expression),answer)
        for expression in ["__import__('os').system('bad')",'(1).__class__','[1,2]','1/0','10**1000','True+1','sqrt(-1)']:
            with self.assertRaises((ValueError,SyntaxError,ArithmeticError,TypeError)):calculate(expression)
        self.assertEqual(results('calc: 1/0',DEFAULTS)[0].kind,'calculator_error')
        self.assertFalse(results('invoice-2026.txt',DEFAULTS))
        self.assertFalse(results('2026-10',DEFAULTS))

    def test_setting_aliases_and_categories(self):
        for query,uri in [('bluetooth','ms-settings:bluetooth'),('settings: wifi','ms-settings:network-wifi'),('sound settings','ms-settings:sound'),('settings storage','ms-settings:storagesense')]:
            hits=results(query,DEFAULTS)
            self.assertTrue(any(r.path==uri for r in hits))
            self.assertTrue(all(r.path in SETTING_URIS for r in hits))
        self.assertFalse(results('bluetooth',dict(DEFAULTS,search_category='Documents')))
        self.assertTrue(results('settings: bluetooth',dict(DEFAULTS,search_category='Documents')))
        self.assertTrue(results('settings:',DEFAULTS))
        self.assertFalse(results('settings: file://example',DEFAULTS))

    def test_quick_actions_bypass_file_index_and_copy_or_launch(self):
        engine=SimpleNamespace(programs=Mock(),search_files=Mock(side_effect=AssertionError('file search used')))
        answer=Engine.search(engine,'2+2',DEFAULTS)[0]
        self.assertEqual(answer.symbol,'4');engine.search_files.assert_not_called()
        owner=SimpleNamespace(quick=Mock(),set_status=Mock(),error=Mock())
        Desktop.open_file(owner,answer);self.assertEqual(QApplication.clipboard().text(),'4')
        Desktop.copy_file(owner,answer);self.assertEqual(QApplication.clipboard().text(),'4')
        settings=Engine.search(engine,'settings: bluetooth',DEFAULTS)[0]
        with patch('desktop.os.startfile',create=True) as start:
            Desktop.open_file(owner,settings);start.assert_called_once_with('ms-settings:bluetooth')
        owner.error.assert_not_called()
        for result in (answer,settings):
            with patch.object(Preview,'text',side_effect=AssertionError('virtual result read as file')):
                preview=Preview(result,embedded=True);preview.close()

    def test_ui_calculator_bypasses_pending_file_worker(self):
        owner=SimpleNamespace(search_sequence=1,settings=DEFAULTS,results=Mock(),quick=Mock(),search_hint=Mock(),pending_search='old',engine=Mock())
        Desktop.request_search(owner,'12*3')
        self.assertIsNone(owner.pending_search)
        self.assertEqual(owner.results.populate.call_args.args[0][0].symbol,'36')
        owner.engine.programs.search.assert_not_called()

if __name__=='__main__':unittest.main()
