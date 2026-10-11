"""Native customization, centered previews, combined log filters and window chrome checks."""
from pathlib import Path
from unittest.mock import patch
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication
from PySide6.QtTest import QTest
from engine import Settings,DEFAULTS
from desktop import Desktop
app=QApplication([]);app.setQuitOnLastWindowClosed(False)
home=Path(__file__).parent/'.smoke/customization';home.mkdir(parents=True,exist_ok=True)
store=Settings(home);store.values=dict(DEFAULTS,auto_model=False,hotkey='Ctrl+Alt+Shift+F24')
window=Desktop(store);window.clock.stop();window.process_timer.stop();window.show();QTest.qWait(80)
assert window.windowFlags()&Qt.WindowType.FramelessWindowHint
for preview in (window.result_preview,window.quick.preview):assert preview.placeholder.alignment()==Qt.AlignmentFlag.AlignCenter
window.navigation.setCurrentRow(1)
window.toggle_sidebar();assert window.sidebar.width()==76 and not window.nav_brand.isVisible()
assert all(window.navigation.item(i).toolTip() for i in range(6))
window.nav_search.click();assert window.navigation.currentRow()==0
window.toggle_sidebar();assert window.sidebar.width()==220
window.log_records=[('INFO','info sentinel'),('WARNING','warning sentinel'),('ERROR','error sentinel'),('CRITICAL','critical sentinel')]
window.set_log_levels(False);window.log_levels['WARNING'].setChecked(True);window.log_levels['ERROR'].setChecked(True)
text=window.log_view.toPlainText();assert 'warning sentinel' in text and 'error sentinel' in text and 'critical sentinel' in text and 'info sentinel' not in text
model=home/'model.log';model.write_text('0.00.001 I model info\n0.00.002 W model warning\n0.00.003 E model error\n')
window.refresh_model_logs(True);text=window.model_log_view.toPlainText();assert 'model warning' in text and 'model error' in text and 'model info' not in text
with model.open('a') as output:output.write('0.00.004 E new live error\n')
window.refresh_model_logs();assert 'new live error' in window.model_log_view.toPlainText()
window.set_log_levels(False);assert not window.log_view.toPlainText() and not window.model_log_view.toPlainText()
for theme in ('Light','Dark'):
 window.widgets['theme'].setCurrentText(theme);window.settings['theme']=theme
 window.widgets['accent_color'].setText('#209b86');window.widgets['popup_tone'].setText('#208b9b')
 window.widgets['popup_transparency'].setValue(60);window.widgets['popup_gradient'].setValue(80)
 assert window.save(quiet=True)
 loaded=Settings(home).values;assert loaded['accent_color']=='#209b86' and loaded['popup_transparency']==60
 window.navigation.setCurrentRow(0);QTest.qWait(60);window.grab().save(str(home/(theme.lower()+'-main.png')))
 with patch.object(window,'request_search'):
  window.quick.summon();QTest.qWait(60);window.quick.grab().save(str(home/(theme.lower()+'-popup.png')));window.quick.hide()
window.showNormal();QTest.qWait(120)
window.window_bar.toggle_maximized();QTest.qWait(150);assert window.window_bar.custom_maximized and window.geometry()==window.screen().availableGeometry()
window.window_bar.toggle_maximized();QTest.qWait(150);assert not window.window_bar.custom_maximized
window.window_bar.controls[0][0].click();QTest.qWait(50);assert window.isMinimized();window.showNormal()
window.reset_appearance();assert window.widgets['accent_color'].text()=='#8573ff'
window.cleanup();window.close()
print('Centered previews, appearance persistence, combined/model log filters, sidebar and custom window controls passed.')
