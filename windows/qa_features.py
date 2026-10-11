"""Native checklist, logging, indexing icon and process shutdown checks."""
import json
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch
import psutil
from PySide6.QtWidgets import QApplication
from PySide6.QtCore import Qt
from PySide6.QtTest import QTest
from engine import Settings,DEFAULTS
from desktop import Desktop
from vectordash_core import server_url
import process_lifecycle

app=QApplication([]);app.setQuitOnLastWindowClosed(False)
home=Path(__file__).parent/'.smoke/new-ui';home.mkdir(parents=True,exist_ok=True)
store=Settings(home);store.values=dict(DEFAULTS,hotkey='Ctrl+Alt+Shift+F21',theme='Light',ocr_enabled=False)
window=Desktop(store);window.clock.stop();window.process_timer.stop();window.show();QTest.qWait(100)
assert window.navigation.item(5).text()=='Logs'
# A shortcut held by another app must not block setup/indexing settings saves.
window.hotkey.unregister()
with patch.object(window.hotkey,'register',side_effect=OSError('Shortcut in use')) as register:
    assert window.save(quiet=True)
    register.assert_not_called()
assert 'unavailable' in window.shortcut_state.text()
window.hotkey.register(window.settings['hotkey'])
window.update_hotkey_status()
window.logger.warning('Skipped sample image: too many pixels');QTest.qWait(30)
window.set_log_levels(False);window.log_levels['WARNING'].setChecked(True);assert 'sample image' in window.log_view.toPlainText()
window.set_log_levels(False);window.log_levels['ERROR'].setChecked(True);assert 'sample image' not in window.log_view.toPlainText()
window.indexing=True;window.animate_index_icon()
red=window.windowIcon().pixmap(64,64).toImage().pixelColor(6,30)
window.engine.paused.set();window.animate_index_icon()
purple=window.windowIcon().pixmap(64,64).toImage().pixelColor(6,30)
assert red.red()>red.blue() and purple.blue()>purple.red(),(red.name(),purple.name())
window.engine.paused.clear();window.index_finished('2 files indexed', '',True,'Manual')
assert window.source_progress.value()==100 and window.source_status.text()=='2 files indexed'
# Master setup uses the chosen model + projector, configures paths and starts.
window.launch=lambda work:work()
model={'label':'Q8_0','projector':{'label':'encoder'}};runtime={'backend':'cpu'}
with patch('desktop.catalogs',return_value=([runtime],[model])),patch('desktop.install_runtime',return_value='H:/managed/llama-server.exe'),patch('desktop.install_model',side_effect=['H:/managed/model-Q8_0.gguf','H:/managed/mmproj-Q8_0.gguf']),patch('desktop.install_ffmpeg',return_value={'ffmpeg_path':'H:/managed/ffmpeg.exe'}),patch.object(window,'start_model') as start:
    window.setup_all()
    assert window.settings['semantic'] and window.settings['projector'].endswith('mmproj-Q8_0.gguf')
    start.assert_called_once()
window.navigation.setCurrentRow(2);QTest.qWait(50)
window.grab().save(str(home/'models-status.png'))
# Real child process cleanup, with a separate helper still running.
window.model_process=process_lifecycle.popen([sys.executable,'-c','import time;time.sleep(120)'],creationflags=subprocess.CREATE_NO_WINDOW)
helper=process_lifecycle.popen([sys.executable,'-c','import time;time.sleep(120)'],creationflags=subprocess.CREATE_NO_WINDOW)
model_pid=window.model_process.pid;helper_pid=helper.pid
window.cleanup()
assert not psutil.pid_exists(model_pid) and not psutil.pid_exists(helper_pid)
with patch('process_lifecycle.subprocess.Popen') as spawn:
    try:process_lifecycle.popen(['new-helper'])
    except InterruptedError:pass
    else:raise AssertionError('Shutdown allowed another helper process')
    spawn.assert_not_called()
window.close()
print('Setup checklist, automatic start, log filters, indexing status/icon and real model/helper shutdown passed.')
