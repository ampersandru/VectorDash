"""Native settings checks for scrolling and automatic setup workflow."""
import json
from pathlib import Path
from unittest.mock import patch
from PySide6.QtCore import Qt,QPoint,QPointF,QTime
from PySide6.QtGui import QWheelEvent
from PySide6.QtWidgets import QApplication,QAbstractSpinBox,QScrollArea,QPushButton
from PySide6.QtTest import QTest
from engine import Settings,DEFAULTS
from desktop import Desktop,ScrollSafeSpinBox,ScrollSafeTimeEdit

app=QApplication([]);app.setQuitOnLastWindowClosed(False)
home=Path(__file__).parent/'.smoke/tool-ui';home.mkdir(parents=True,exist_ok=True)
store=Settings(home);store.values=dict(DEFAULTS,hotkey='Ctrl+Alt+Shift+F22',theme='Light')
window=Desktop(store);window.clock.stop();window.process_timer.stop();window.show()
QTest.qWait(100)
model_page=window.stack.widget(2)
for key in ('ocr_enabled','ocr_path','ocr_data','ocr_languages','ffmpeg_path','projector'):
    assert model_page.isAncestorOf(window.widgets[key]),key

for page,key in [(1,'video_interval'),(3,'cpu_limit'),(3,'gpu_limit'),(3,'idle_minutes'),(3,'schedule_time')]:
    window.navigation.setCurrentRow(page);QTest.qWait(20)
    widget=window.widgets[key]
    scroll=window.stack.widget(page).findChild(QScrollArea)
    scroll.ensureWidgetVisible(widget)
    widget.setFocus()
    before=widget.time() if isinstance(widget,ScrollSafeTimeEdit) else widget.value()
    for amount in (120,-120):
        local=QPoint(5,5)
        event=QWheelEvent(QPointF(local),QPointF(widget.mapToGlobal(local)),QPoint(),QPoint(0,amount),Qt.MouseButton.NoButton,Qt.KeyboardModifier.NoModifier,Qt.ScrollPhase.NoScrollPhase,False)
        app.sendEvent(widget.lineEdit(),event)
        assert (widget.time() if isinstance(widget,ScrollSafeTimeEdit) else widget.value())==before,key
print('Focused numeric/time fields keep their values when wheel scrolling through settings.')
widget=window.widgets['cpu_limit'];scroll=window.stack.widget(3).findChild(QScrollArea)
scroll.ensureWidgetVisible(widget);bar=scroll.verticalScrollBar()
bar.setValue(bar.maximum()//2);before=bar.value()
event=QWheelEvent(QPointF(5,5),QPointF(widget.mapToGlobal(QPoint(5,5))),QPoint(),QPoint(0,-120),Qt.MouseButton.NoButton,Qt.KeyboardModifier.NoModifier,Qt.ScrollPhase.NoScrollPhase,False)
app.sendEvent(widget,event)
assert bar.value()!=before,(before,bar.value())
print('Wheel scrolling over a numeric field still scrolls the settings page.')
# Keyboard editing remains available.
widget.setFocus();value=widget.value();QTest.keyClick(widget,Qt.Key.Key_Up)
assert widget.value()==min(widget.maximum(),value+1)
print('Keyboard arrows still change numeric values intentionally.')
# No catalog needs to be loaded before an encoder setup button works.
window.models=[];window.runtimes=[]
window.quant_choice.setCurrentIndex(1)
model={'label':'BF16','name':'model.gguf','size':1,'projector':{'name':'mmproj.gguf','label':'Vision/audio'}}
runtime={'label':'CPU','backend':'cpu','size':1}
with patch('desktop.catalogs',return_value=([runtime],[model])),patch('desktop.install_model',return_value='H:/verified/mmproj.gguf') as install:
    window.launch=lambda work:work()
    window.install('projector')
    assert install.call_args.args[0]['name']=='mmproj.gguf'
    assert window.settings['projector']=='H:/verified/mmproj.gguf'
    assert window.quant_choice.currentData()=='BF16'
print('One-click encoder setup loads its catalog, preserves quant selection and saves its path.')
with patch('desktop.install_ffmpeg',return_value={'ffmpeg_path':'H:/verified/ffmpeg.exe'}):
    window.install_tool('ffmpeg')
    assert window.settings['ffmpeg_path']=='H:/verified/ffmpeg.exe'
print('One-click tool setup saves its configured executable path.')
window.hotkey.unregister();window.tray.hide();window.shutting_down=True;window.close()
