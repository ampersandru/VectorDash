"""Preview information below content, latest logs and rounded dropdown checks."""
from pathlib import Path
from unittest.mock import patch
from PIL import Image
from PySide6.QtCore import Qt,QPoint
from PySide6.QtWidgets import QApplication
from PySide6.QtTest import QTest
from desktop import Desktop
from engine import Settings,DEFAULTS,SearchResult
app=QApplication([]);app.setStyle('Fusion');app.setQuitOnLastWindowClosed(False)
home=Path(__file__).parent/'.smoke/preview-logs';home.mkdir(parents=True,exist_ok=True)
store=Settings(home);store.values=dict(DEFAULTS,auto_model=False,hotkey='Ctrl+Alt+Shift+F20')
patch.object(Desktop,'refresh_component_status',return_value=None).start()
patch.object(Desktop,'probe_model',return_value=None).start()
window=Desktop(store);window.clock.stop();window.process_timer.stop();window.show()
window.log_records=[('INFO',f'event {i}') for i in range(500)];window.render_logs()
window.navigation.setCurrentRow(5);QTest.qWait(30)
bar=window.log_view.verticalScrollBar();assert bar.maximum()>0 and bar.value()==bar.maximum()
bar.setValue(0);window.navigation.setCurrentRow(0);window.navigation.setCurrentRow(5);QTest.qWait(30)
assert bar.value()==bar.maximum()
model=home/'model.log';model.write_text(''.join(f'0.00.001 I model {i}\n' for i in range(500)))
window.log_tabs.setCurrentIndex(1);QTest.qWait(30);bar=window.model_log_view.verticalScrollBar()
assert bar.maximum()>0 and bar.value()==bar.maximum()
photo=home/'unique-filename.png';Image.new('RGB',(200,100),'purple').save(photo)
result=SearchResult(.85,str(photo),'dog',kind='image',reasons=['Matches the picture'])
window.navigation.setCurrentRow(0)
for pane in (window.result_preview,window.quick.preview):
 pane.show_result(result);preview=pane.current
 assert preview.layout.indexOf(preview.scroll)<preview.layout.indexOf(preview.caption)<preview.layout.indexOf(preview.match_info)
 assert photo.name not in preview.caption.text() and photo.name not in preview.windowTitle()
with patch.object(window,'request_search'):window.quick.summon()
QTest.qWait(200)
for theme in ('Light','Dark'):
 window.settings['theme']=theme;window.apply_theme()
 with patch.object(window,'request_search'):
  QTest.qWait(80)
  combo=window.quick.filter_category;combo.setFocus();QTest.qWait(50);combo.showPopup();QTest.qWait(30)
  container=combo.view().window();assert container.isVisible() and window.quick.isVisible()
  assert not container.mask().isEmpty() and not container.mask().contains(QPoint(0,0))
  container.grab().save(str(home/(theme.lower()+'-dropdown.png')))
  combo.hidePopup();window.quick.grab().save(str(home/(theme.lower()+'-popup.png')))
print('Latest logs, preview info order/filename removal and rounded light/dark popup dropdowns passed.')
window.shutting_down=True;window.hotkey.unregister();window.quick.hide();window.hide()
