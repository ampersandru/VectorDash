"""Broad filename searches must render directory matches without missing assets."""
from pathlib import Path
import unittest
from unittest.mock import Mock,patch
from PySide6.QtWidgets import QApplication,QWidget
from engine import SearchResult
from desktop import Results
from appearance import icon

class FolderResultTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.app=QApplication.instance() or QApplication([])

    def test_folder_first_results_render_in_main_and_popup(self):
        owner=QWidget();owner.settings={};owner.open_file=Mock()
        owner.preview_result=Mock();owner.copy_file=Mock()
        folder=SearchResult(2,'C:/Games/Forza','',kind='folder')
        file=SearchResult(1.4,'C:/Games/Forza/forzahorizon6.exe','',kind='filename')
        for _ in range(2):
            results=Results(owner);results.populate([folder,file])
            self.assertEqual(results.count(),2)
            self.assertEqual(results.selected_result().kind,'folder')
            self.assertIsNotNone(results.itemWidget(results.item(0)))
            results.close()
        owner.close()

    def test_missing_optional_icon_cannot_abort_result_display(self):
        with self.assertLogs('vectordash',level='WARNING'):
            missing=icon('not-a-bundled-icon')
        self.assertTrue(missing.isNull())
        self.assertFalse(icon('folder-multiple').isNull())

if __name__=='__main__':unittest.main()
