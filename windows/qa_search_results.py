"""Native compact rows, shared asynchronous thumbnails and matching toggle checks."""
import time
from unittest.mock import patch
from PySide6.QtCore import Qt
from pathlib import Path
from PySide6.QtWidgets import QApplication
from PySide6.QtTest import QTest
from PySide6.QtGui import QImage,QColor
from engine import Settings,DEFAULTS,SearchResult
from desktop import Desktop
app=QApplication([]);app.setQuitOnLastWindowClosed(False)
home=Path(__file__).parent/'.smoke/search-results';home.mkdir(parents=True,exist_ok=True)
store=Settings(home);store.values=dict(DEFAULTS,auto_model=False,hotkey='Ctrl+Alt+Shift+F22')
window=Desktop(store);window.clock.stop();window.process_timer.stop();window.show()
photo=home/'dog.png';image=QImage(300,200,QImage.Format.Format_RGB32);image.fill(QColor('#ab5511'));image.save(str(photo))
results=[SearchResult(.8,str(photo),'OCR text should stay in preview',kind='image',embedding_source='media')]
start=time.perf_counter()
for pane in (window.results,window.quick.results):pane.populate(results)
for _ in range(100):
 app.processEvents();QTest.qWait(10)
 if window.thumbnails.cache:break
assert window.thumbnails.cache
for pane in (window.results,window.quick.results):
 assert pane.item(0).sizeHint().height()==66
 icon=next(iter(pane.thumbnail_labels.values()));assert not icon.pixmap().isNull()
 assert icon.pixmap().width()==56
assert not window.name_filters[0].isChecked()
window.name_filters[0].setChecked(True)
assert all(check.isChecked() for check in window.name_filters)
assert Settings(home).values['search_names']
window.name_filters[1].setChecked(False)
assert not any(check.isChecked() for check in window.name_filters)
start=time.perf_counter();window.results.populate(results*100);elapsed=(time.perf_counter()-start)*1000
print(f'100 compact cached-thumbnail rows displayed in {elapsed:.1f} ms; shared thumbnail and toggles passed.')
with patch.object(window,'request_search'):
 window.query.setFocus();window.filter_category.setCurrentIndex(0)
 QTest.keyClick(window.query,Qt.Key.Key_Tab)
 assert window.filter_category.currentIndex()==1 and window.quick.filter_category.currentIndex()==1
 QTest.keyClick(window.results,Qt.Key.Key_Tab)
 assert window.filter_category.currentIndex()==2
 QTest.keyClick(window.query,Qt.Key.Key_Backtab)
 assert window.filter_category.currentIndex()==1
 window.filter_category.setCurrentIndex(window.filter_category.count()-1)
 QTest.keyClick(window.query,Qt.Key.Key_Tab);assert window.filter_category.currentIndex()==0
 window.quick.show();window.quick.activateWindow();window.quick.input.setFocus();QTest.qWait(20)
 QTest.keyClick(window.quick.input,Qt.Key.Key_Tab)
 assert window.quick.filter_category.currentIndex()==1 and window.filter_category.currentIndex()==1
 window.quick.hide();window.navigation.setCurrentRow(4)
 QTest.keyClick(window.query,Qt.Key.Key_Tab)
 assert window.filter_category.currentIndex()==1
 window.navigation.setCurrentRow(0)
 assert window.search_shortcut_help.alignment() & Qt.AlignmentFlag.AlignRight
 assert window.quick.shortcut_help.alignment() & Qt.AlignmentFlag.AlignRight
 for hint in (window.search_shortcut_help,window.quick.shortcut_help):assert '<b>Tab</b> File type' in hint.text() and 'Ctrl+C' in hint.text()
 window.search_finished(window.search_sequence,results,'')
 assert '<b>Tab</b> File type' in window.quick.shortcut_help.text()
print('Main/popup Tab and Shift+Tab cycling, wraparound, settings isolation and persistent right-aligned shortcut hints passed.')
window.shutting_down=True;window.hotkey.unregister();window.quick.hide();window.hide()
