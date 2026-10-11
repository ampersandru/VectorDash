"""Native compact rows, shared asynchronous thumbnails and matching toggle checks."""
import time
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
window.shutting_down=True;window.hotkey.unregister();window.quick.hide();window.hide()
