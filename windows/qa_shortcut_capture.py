"""Native shortcut capture, cancellation, bold hints and registration lifecycle."""
from pathlib import Path
from unittest.mock import patch
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication
from PySide6.QtTest import QTest
from engine import Settings,DEFAULTS
from desktop import Desktop,ShortcutRecorder
app=QApplication([]);app.setQuitOnLastWindowClosed(False)
home=Path(__file__).parent/'.smoke/shortcut-capture';home.mkdir(parents=True,exist_ok=True)
store=Settings(home);store.values=dict(DEFAULTS,auto_model=False,hotkey='Ctrl+Alt+Shift+F22')
patch.object(Desktop,'refresh_component_status',return_value=None).start()
window=Desktop(store);window.clock.stop();window.process_timer.stop();window.show();window.navigation.setCurrentRow(4)
recorder=window.widgets['hotkey'];assert isinstance(recorder,ShortcutRecorder)
recorder.setFocus();QTest.qWait(30);assert not window.hotkey.current
QTest.keyClick(recorder,Qt.Key.Key_F19,Qt.KeyboardModifier.ControlModifier | Qt.KeyboardModifier.AltModifier)
assert recorder.text()=='Ctrl+Alt+F19'
window.widgets['start_hidden'].setFocus();QTest.qWait(10)
assert window.hotkey.current=='Ctrl+Alt+Shift+F22'
assert window.save(quiet=True);assert window.hotkey.current=='Ctrl+Alt+F19'
assert Settings(home).values['hotkey']=='Ctrl+Alt+F19'
recorder.setFocus();QTest.qWait(10)
QTest.keyClick(recorder,Qt.Key.Key_K,Qt.KeyboardModifier.MetaModifier)
assert recorder.text()=='Win+K'
QTest.keyClick(recorder,Qt.Key.Key_Escape)
assert recorder.text()=='Ctrl+Alt+F19' and window.hotkey.current=='Ctrl+Alt+F19'
recorder.setFocus();QTest.keyClick(recorder,Qt.Key.Key_X)
assert recorder.text()=='Ctrl+Alt+F19'
QTest.keyClick(recorder,Qt.Key.Key_Tab,Qt.KeyboardModifier.ControlModifier)
assert recorder.text()=='Ctrl+Tab'
QTest.keyClick(recorder,Qt.Key.Key_Escape)
for hint in (window.search_shortcut_help,window.quick.shortcut_help):
 assert hint.textFormat()==Qt.TextFormat.RichText
 assert '<b>Ctrl+C</b> Copy file' in hint.text() and '<b>Tab</b> File type' in hint.text()
print('Shortcut capture, Win/modifier keys, Tab capture, cancellation, saved registration and bold hints passed.')
window.shutting_down=True;window.hotkey.unregister();window.quick.hide();window.hide()
