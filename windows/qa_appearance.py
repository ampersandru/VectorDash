"""Theme rendering, idle progress and right-click file exclusion checks."""
from pathlib import Path
from unittest.mock import patch
from PIL import Image
from PySide6.QtCore import Qt,QEvent,QTimer
from PySide6.QtGui import QContextMenuEvent
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication
from desktop import Desktop
from engine import Settings,DEFAULTS,SearchResult
app=QApplication([]);app.setQuitOnLastWindowClosed(False)
home=Path(__file__).parent/'.smoke/appearance';home.mkdir(parents=True,exist_ok=True)
image=home/'photo.png';Image.new('RGB',(200,150),'purple').save(image)
store=Settings(home);store.values=dict(DEFAULTS,auto_model=False,hotkey='Ctrl+Alt+Shift+F23',mcp_token='appearance_token_12345678901234567890')
window=Desktop(store);window.clock.stop();window.process_timer.stop();window.show()
with patch.object(window,'request_search'):
 for theme in ('Light','Dark'):
  window.widgets['theme'].setCurrentText(theme);window.settings['theme']=theme;window.apply_theme()
  window.quick.summon();QTest.qWait(100)
  picture=window.quick.grab().toImage();left=picture.pixelColor(12,100);right=picture.pixelColor(picture.width()-12,400)
  assert 0<left.alpha()<255,(theme,left.getRgb())
  assert left!=right,(theme,left.getRgb(),right.getRgb())
  window.quick.grab().save(str(home/(theme.lower()+'-popup.png')))
  window.quick.hide();window.navigation.setCurrentRow(5);QTest.qWait(80)
  color=window.log_view.grab().toImage().pixelColor(window.log_view.width()-20,20).name()
  assert color==('#20212c' if theme=='Dark' else '#ffffff'),(theme,color)
  window.navigation.setCurrentRow(2);window.update_tool_state();QTest.qWait(50)
  assert window.setup_progress.isHidden() and window.download_bar.isHidden()
  window.installing=True;window.update_tool_state();assert not window.setup_progress.isHidden()
  window.installing=False;window.update_tool_state();assert window.setup_progress.isHidden()
 result=SearchResult(.7,str(image),'',kind='image')
 window.results.populate([result]);window.quick.summon();window.quick.results.populate([result]);QTest.qWait(60)
 def choose():
  menu=app.activePopupWidget()
  assert menu
  app.sendEvent(window.quick,QEvent(QEvent.Type.WindowDeactivate))
  assert window.quick.isVisible()
  menu.setActiveAction(menu.actions()[0]);QTest.keyClick(menu,Qt.Key.Key_Return)
 point=window.quick.results.visualItemRect(window.quick.results.item(0)).center()
 event=QContextMenuEvent(QContextMenuEvent.Reason.Mouse,point,window.quick.results.mapToGlobal(point))
 QTimer.singleShot(80,choose)
 window.quick.results.contextMenuEvent(event)
 assert window.quick.isVisible() and not window.quick.context_menu_open
 assert window.quick.results.count()==0 and window.results.count()==0
 assert len(Settings(home).values['search_excluded_files'])==1
 window.excluded_search_files.setCurrentRow(0);window.restore_search_file()
 assert not Settings(home).values['search_excluded_files']
window.cleanup();window.close()
print('Popup alpha/gradient, dark logs, idle progress and exclusion/restore passed.')
