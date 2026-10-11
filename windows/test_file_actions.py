"""File actions never preview on Enter or delete without an explicit choice."""
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch
from PySide6.QtWidgets import QApplication, QWidget, QMenu
from PySide6.QtCore import Qt
from PySide6.QtGui import QContextMenuEvent
from PySide6.QtTest import QTest
from desktop import Desktop, Results
from engine import SearchResult

class FileActionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app=QApplication.instance() or QApplication([])

    def test_open_image_pdf_media_and_code_use_windows_association(self):
        owner=SimpleNamespace(quick=Mock(),error=Mock(),show_large_preview=Mock())
        for kind,page,seconds in [('image',0,-1),('document',3,-1),('video',0,12),('code',0,-1)]:
            result=SearchResult(1,'C:/example.file','')
            result.kind=kind;result.page=page;result.seconds=seconds
            with patch('desktop.os.startfile') as opened:
                Desktop.open_file(owner,result)
                opened.assert_called_once_with(result.path)
        owner.show_large_preview.assert_not_called()

    def test_both_result_lists_offer_actions_and_enter_opens_space_previews(self):
        owner=QWidget();owner.settings={};owner.quick=Mock();owner.quick.isVisible.return_value=False
        owner.shutting_down=False
        for name in ('open_file','exclude_search_file','reveal_file','delete_file','copy_file','file_properties','preview_result'):
            setattr(owner,name,Mock())
        labels=['Open file','Exclude file from future searches','Open file location','Delete...','Copy','Properties']
        methods=['open_file','exclude_search_file','reveal_file','delete_file','copy_file','file_properties']
        for popup in (False,True):
            owner.quick.isVisible.return_value=popup
            results=Results(owner);results.resize(400,200);results.show()
            results.populate([(1,'C:/example.txt','')]);self.app.processEvents()
            QTest.keyClick(results,Qt.Key.Key_Return)
            owner.open_file.assert_called_with('C:/example.txt')
            QTest.keyClick(results,Qt.Key.Key_Space)
            owner.preview_result.assert_called_with('C:/example.txt')
            for index,method in enumerate(methods):
                def choose(menu,position):
                    self.assertEqual([action.text() for action in menu.actions()],labels)
                    return menu.actions()[index]
                point=results.visualItemRect(results.item(0)).center()
                event=QContextMenuEvent(QContextMenuEvent.Reason.Mouse,point,results.mapToGlobal(point))
                def make_menu(parent):
                    menu=QMenu(parent);menu.exec=lambda position:choose(menu,position)
                    return menu
                with patch('desktop.QMenu',side_effect=make_menu):results.contextMenuEvent(event)
                getattr(owner,method).assert_called_with('C:/example.txt')
                self.assertFalse(owner.quick.context_menu_open)
            results.close()
        owner.close()

    def test_delete_choices_cancel_recycle_and_permanent(self):
        with tempfile.TemporaryDirectory() as directory:
            file=Path(directory)/'example.txt'
            for choice in ['cancel','trash','permanent']:
                file.write_text('temporary test fixture')
                quick=Mock();quick.isVisible.return_value=False;quick.results.count.return_value=0
                results=Mock();results.count.return_value=0
                owner=SimpleNamespace(quick=quick,results=results,winId=lambda:0,set_status=Mock(),error=Mock())
                dialog=Mock();trash=object();permanent=object();cancel=object()
                dialog.addButton.side_effect=[trash,permanent,cancel]
                dialog.clickedButton.return_value={'cancel':cancel,'trash':trash,'permanent':permanent}[choice]
                with patch('desktop.QMessageBox') as box,patch('desktop.recycle_file',return_value=True) as recycle:
                    box.return_value=dialog
                    Desktop.delete_file(owner,str(file))
                    dialog.exec.assert_called_once()
                    dialog.setDefaultButton.assert_called_once_with(cancel)
                    dialog.setEscapeButton.assert_called_once_with(cancel)
                    if choice=='trash':recycle.assert_called_once_with(str(file),0)
                    else:recycle.assert_not_called()
                self.assertEqual(file.exists(),choice!='permanent')
                owner.error.assert_not_called()

if __name__=='__main__':unittest.main()
