"""Native quick-search preview return, filters and MCP settings smoke check."""
from pathlib import Path
from unittest.mock import patch
from PIL import Image
from PySide6.QtCore import Qt
from PySide6.QtTest import QTest
from PySide6.QtWidgets import QApplication
from desktop import Desktop
from engine import Settings,DEFAULTS,SearchResult

app=QApplication([]);app.setQuitOnLastWindowClosed(False)
home=Path(__file__).parent/'.smoke/v02-ui';home.mkdir(parents=True,exist_ok=True)
image=home/'dog.png';Image.new('RGB',(100,100),'tan').save(image)
store=Settings(home);store.values=dict(DEFAULTS,theme='Light',hotkey='Ctrl+Alt+Shift+F21',mcp_token='test_token_12345678901234567890')
window=Desktop(store);window.clock.stop();window.process_timer.stop();window.show()
with patch.object(window,'request_search'):
    window.quick.summon();QTest.qWait(80)
    result=SearchResult(.73,str(image),'',kind='image',query='dog',reasons=['Matches the picture'])
    window.quick.results.populate([result]);window.quick.results.setFocus();QTest.qWait(40)
    assert '0.730' in window.quick.preview.current.match_info.text()
    assert 'Matches the picture' in window.quick.preview.current.match_info.text()
    QTest.keyClick(window.quick.results,Qt.Key.Key_Space);QTest.qWait(100)
    assert window.quick.isVisible() and window.large_preview.isVisible() and window.quick.preview_open
    closed=[]
    window.large_preview.finished.connect(lambda *args:closed.append(True))
    QTest.keyClick(window.large_preview,Qt.Key.Key_Space);QTest.qWait(100)
    assert window.quick.isVisible() and not window.quick.preview_open
    assert closed
    window.widgets['mcp_auth'].setChecked(False)
    window.copy_mcp()
    import json
    assert 'headers' not in json.loads(app.clipboard().text())
    window.widgets['mcp_lan'].setChecked(True)
    with patch('mcp_network.lan_ip',return_value='192.168.1.42'):
        window.copy_mcp()
    assert json.loads(app.clipboard().text())['url']=='http://192.168.1.42:8790/mcp'
    window.widgets['mcp_lan'].setChecked(False)
    window.quick.hide();window.show_large_preview(result);QTest.qWait(80)
    closed=[];window.large_preview.finished.connect(lambda *args:closed.append(True))
    QTest.keyClick(window.large_preview.caption,Qt.Key.Key_Space);QTest.qWait(80)
    assert closed and window.isVisible()
    window.quick.summon()
    window.quick.filter_excludes.setText('.zip, .log')
    assert window.filter_excludes.text()=='.zip, .log'
    window.filter_category.setCurrentText('Photos / screenshots')
    assert window.quick.filter_category.currentText()=='Photos / screenshots'
    window.settings['mcp_auth']=True;window.settings['mcp_enabled']=True;window.settings['mcp_port']=18792
    window.apply_mcp();assert window.mcp and 'Bearer required' in window.mcp_status.text()
    window.settings['mcp_enabled']=False;window.apply_mcp();assert window.mcp is None
    window.quick.grab().save(str(home/'quick.png'))
window.cleanup();window.close()
print('Native UI: match information, Space preview return, shared filters and MCP start/stop passed.')
