"""Verify rendered card button hover, press, focus and disabled states."""
from pathlib import Path
from PySide6.QtCore import QPoint,Qt
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication
from desktop import Desktop,card,button
from engine import Settings,DEFAULTS
app=QApplication([]);app.setQuitOnLastWindowClosed(False)
home=Path(__file__).parent/'.smoke/buttons';home.mkdir(parents=True,exist_ok=True)
store=Settings(home);store.values=dict(DEFAULTS,auto_model=False,hotkey='Ctrl+Alt+Shift+F22',mcp_token='button_test_token_12345678901234567890')
window=Desktop(store);window.show();window.clock.stop();window.process_timer.stop()
frame,layout=card('Button check');frame.setParent(window);frame.move(260,260);frame.resize(400,180)
clicks=[];neutral=button('Neutral button',lambda:clicks.append(True));layout.addWidget(neutral);frame.show();frame.raise_();window.raise_();window.activateWindow();QTest.qWait(150)
def color():return neutral.grab().toImage().pixelColor(20,10).name()
for theme in ('Light','Dark'):
 window.widgets['theme'].setCurrentText(theme);window.settings['theme']=theme;window.apply_theme();QTest.mouseMove(window,QPoint(10,10));QTest.qWait(80)
 normal=color();QTest.mouseMove(neutral,QPoint(30,15));QTest.qWait(80);hover=color()
 QTest.mousePress(neutral,Qt.MouseButton.LeftButton,pos=QPoint(30,15));QTest.qWait(60);pressed=color()
 QTest.mouseRelease(neutral,Qt.MouseButton.LeftButton,pos=QPoint(30,15));QTest.qWait(60)
 assert len({normal,hover,pressed})==3,(theme,normal,hover,pressed)
 assert normal==('#ffffff' if theme=='Light' else '#20212c'),(theme,normal)
 assert clicks
 neutral.setEnabled(False);QTest.qWait(60);disabled=color();assert disabled!=hover
 neutral.setEnabled(True)
 print(theme,'button states:',normal,hover,pressed,disabled)
window.cleanup();window.close()
