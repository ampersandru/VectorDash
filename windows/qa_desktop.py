"""Interactive Windows smoke checks; uses isolated settings and no startup registry writes."""
import ctypes, json, os, subprocess, sys, tempfile, time
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).parent))
from PySide6.QtCore import QEvent, QTimer, Qt
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication
from engine import Settings
from desktop import Desktop
from windows_integration import GlobalHotkey, HOTKEY_ID

app=QApplication([])
app.setQuitOnLastWindowClosed(False)
root=Path(__file__).parent/'.smoke'/'interaction-data'
store=Settings(root)
store.values.update({'hotkey':'Ctrl+Alt+Shift+F24','theme':'Light','startup':False,'schedule_mode':'Manual'})
window=Desktop(store)
window.clock.stop()
window.process_timer.stop()
window.show()
app.processEvents()
# Allow the one-time startup telemetry request to finish before injecting test metrics.
QTest.qWait(800)
app.processEvents()

def wait_for(condition,seconds=5):
    until=time.monotonic()+seconds
    while time.monotonic()<until:
        app.processEvents()
        if condition(): return True
        QTest.qWait(20)
    return False

# Verify the real Windows message pump delivers WM_HOTKEY into Qt.
window.quick.hide()
assert ctypes.windll.user32.PostThreadMessageW(ctypes.windll.kernel32.GetCurrentThreadId(),0x0312,HOTKEY_ID,0)
assert wait_for(lambda:window.quick.isVisible()), 'Native shortcut did not summon search'
QTest.keyClick(window.quick,Qt.Key.Key_Escape)
assert wait_for(lambda:not window.quick.isVisible()),'Escape did not dismiss search'
print('Native global shortcut message and Escape passed')

# Another app reserving the combination must fail rather than silently replace it.
other=GlobalHotkey()
try:
    other.register('Ctrl+Alt+Shift+F24')
    raise AssertionError('Shortcut conflict was not detected')
except OSError:
    pass
print('Global shortcut conflict detection passed')

window.devices_finished([('CUDA0','NVIDIA RTX'),('Vulkan1','Intel UHD')],'')
window.devices.setCurrentIndex(2)
window.widgets['mode'].setCurrentText('Integrated GPU')
assert window.save(quiet=True)
assert store.values['device']=='Vulkan1'
print('Selectable device saved the exact runtime identifier')

with patch('desktop.set_startup') as startup:
    window.widgets['startup'].setChecked(True)
    assert window.save(quiet=True)
    startup.assert_called_once_with(True)
window.widgets['startup'].setChecked(False)
with patch('desktop.set_startup'):
    window.save(quiet=True)
print('Startup preference integration passed (isolated, no registry changes)')

window.settings['close_behavior']='Hide to tray'
window.close()
assert not window.isVisible()
window.show_main()
assert window.isVisible()
window.settings['close_behavior']='Minimize to taskbar'
window.close()
assert window.isMinimized()
window.show_main()
window.settings['minimize_behavior']='Tray'
window.showMinimized()
assert wait_for(lambda:not window.isVisible())
window.show_main()
print('Close to tray, close to taskbar, restore and minimize-to-tray passed')

window.quick.summon()
window.quick.results.populate([(1.0,'C:/example.txt','A searchable example')])
with patch('desktop.os.startfile') as opened:
    QTest.keyClick(window.quick.input,Qt.Key.Key_Return)
    opened.assert_called_once_with('C:/example.txt')
print('Quick-search Enter opens the selected result')

# Drive the actual scheduler controller/worker with a synthetic source.
assert wait_for(lambda:not window.collecting), 'Startup telemetry did not finish'
with tempfile.TemporaryDirectory(prefix='digup-ui-',dir=Path(__file__).parents[1]) as directory:
    source=Path(directory)
    (source/'notes.txt').write_text('Scheduling a local Windows index',encoding='utf-8')
    window.settings.update(roots=[str(source)],semantic=False,schedule_mode='When idle',check_user_idle=False,check_gpu=False,check_cpu=True,last_success='',cpu_limit=25)
    window.metrics={'cpu':5,'gpu':None,'idle':0,'on_ac':True}
    window.metrics_time=time.monotonic()
    window.metrics_finished(window.metrics)
    assert wait_for(lambda:not window.indexing), 'Scheduled indexing did not finish'
    assert window.settings['last_success'], 'Scheduled success was not recorded'
    assert window.engine.search('Scheduling',window.settings)
    window.metrics={'cpu':95,'gpu':None,'idle':0,'on_ac':True}
    window.metrics_time=time.monotonic()
    window.start_index(scheduled=True,slot='test-slot')
    QTest.qWait(100)
    assert window.indexing, 'Resource-conditioned indexing did not wait'
    window.widgets['schedule_mode'].setCurrentText('Manual')
    assert window.save(quiet=True)
    assert wait_for(lambda:not window.indexing), 'Disabling schedule did not stop the waiting job'
    print('Automatic scheduling, result commit, CPU waiting and schedule-disable cancellation passed')

window.shutting_down=True
window.hotkey.unregister()
window.tray.hide()
window.quick.hide()
window.close()
print('Desktop interaction checks passed')
