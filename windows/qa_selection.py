"""Native selection preview, selective setup, status colors and Explorer checks."""
from pathlib import Path
from unittest.mock import patch
from PIL import Image
from PySide6.QtWidgets import QApplication,QPlainTextEdit
from PySide6.QtCore import Qt
from PySide6.QtTest import QTest
from engine import Settings,DEFAULTS,SearchResult
from desktop import Desktop
app=QApplication([]);app.setQuitOnLastWindowClosed(False)
home=Path(__file__).parent/'.smoke/selection-ui';home.mkdir(parents=True,exist_ok=True)
image=home/'dog photo.png';Image.new('RGB',(640,400),'tan').save(image)
text=home/'notes.txt';text.write_text('Invoice INV-99 and payment declined')
store=Settings(home);store.values=dict(DEFAULTS,hotkey='Ctrl+Alt+Shift+F20',theme='Light',ocr_enabled=False)
window=Desktop(store);window.clock.stop();window.process_timer.stop();window.show();QTest.qWait(60)
assert 'semantic' not in window.widgets and window.settings['semantic']
assert window.quick.input.actions() and not window.quick.input.actions()[0].icon().isNull()
assert 'QScrollBar::handle:vertical' in app.styleSheet() and 'border-radius: 4px' in app.styleSheet()
photo=SearchResult(1,str(image),'',kind='image',query='dogs')
note=SearchResult(.8,str(text),'Invoice INV-99',query='INV-99')
window.results.populate([photo,note]);QTest.qWait(40)
preview=window.result_preview.current
assert preview and not preview.picture.pixmap().isNull()
assert window.result_preview.geometry().left()>window.results.geometry().left()
window.results.setCurrentRow(1);QTest.qWait(30)
assert preview.released
viewer=window.result_preview.current.findChild(QPlainTextEdit)
assert viewer and 'INV-99' in viewer.toPlainText() and viewer.isReadOnly()
fixtures=Path(__file__).parent/'.smoke/multimedia/source'
if (fixtures/'009.pdf').exists():
    pdf=SearchResult(1,str((fixtures/'009.pdf').resolve()),'pets',kind='pdf',page=2,query='pets')
    window.results.populate([pdf]);QTest.qWait(30)
    assert window.result_preview.current.page.value()==2
    assert window.result_preview.current.highlight_count>0
if (fixtures/'004.mp4').exists():
    from PySide6.QtMultimedia import QMediaPlayer
    media=SearchResult(1,str((fixtures/'004.mp4').resolve()),'',kind='video',seconds=9.2)
    window.results.populate([media])
    player_preview=window.result_preview.current
    for _ in range(100):
        QTest.qWait(30)
        if hasattr(player_preview,'opened_position'):break
    assert player_preview.opened_position==9200
    assert player_preview.player.playbackState()!=QMediaPlayer.PlaybackState.PlayingState
    window.results.populate([photo,note]);QTest.qWait(30)
    assert player_preview.released
window.results.populate([photo,note]);QTest.qWait(30)
window.results.setCurrentRow(0);QTest.qWait(30)
window.results.setFocus()
QTest.keyClick(window.results,Qt.Key.Key_C,Qt.KeyboardModifier.ControlModifier)
clipboard=app.clipboard().mimeData()
assert clipboard.hasUrls() and Path(clipboard.urls()[0].toLocalFile())==image.resolve()
import ctypes
assert ctypes.windll.user32.IsClipboardFormatAvailable(15), 'Missing Windows file-copy format CF_HDROP'
from previews import Preview
QTest.keyClick(window.results,Qt.Key.Key_Space);QTest.qWait(30)
large=window.findChildren(Preview,options=Qt.FindChildOption.FindDirectChildrenOnly)
assert large and large[-1].isVisible() and not large[-1].embedded
large[-1].close();QTest.qWait(20)
window.grab().save(str(home/'search-preview.png'))
with patch.object(window,'request_search'):
    window.quick.summon()
    window.quick.results.populate([photo,note]);QTest.qWait(30)
    assert window.quick.preview.current and not window.quick.preview.current.picture.pixmap().isNull()
    popup=window.quick.grab()
    popup.save(str(home/'quick-preview.png'))
    assert popup.toImage().pixelColor(0,0).alpha()==0
    border=popup.toImage().pixelColor(3,popup.height()//2)
    assert border.blue()>border.red() and border.alpha()>200, border.name()
    with patch.object(window,'preview_result') as preview_action:
        QTest.keyClick(window.quick.results,Qt.Key.Key_Space)
        preview_action.assert_called_once_with(photo)
    QTest.keyClick(window.quick.results,Qt.Key.Key_C,Qt.KeyboardModifier.ControlModifier)
    assert Path(app.clipboard().mimeData().urls()[0].toLocalFile())==image.resolve()
    quick_preview=window.quick.preview.current
    window.quick.hide();QTest.qWait(30)
    assert quick_preview.released and window.quick.preview.current is None
with patch('desktop.reveal_in_explorer') as reveal:
    window.reveal();reveal.assert_called_once_with(str(image))
for key,name in [('server','llama-server.exe'),('model','embeddinggemma-2-Q8_0.gguf')]:
    path=home/name;path.write_bytes(b'fixture')
    window.settings[key]=str(path);window.widgets[key].setText(str(path))
for key in ('ocr','projector','ffmpeg'):window.setup_choices[key].setChecked(False)
window.update_tool_state()
assert window.setup_button.text()=='Start'
assert window.setup_cancel.isHidden()
window.installing=True;window.update_tool_state();assert not window.setup_cancel.isHidden()
window.installing=False;window.update_tool_state()
assert '#aa7900' in window.component_status['model'].styleSheet()
assert window.component_status['model'].alignment() & Qt.AlignmentFlag.AlignRight
window.model_status('loaded · text: ready')
assert '#16834a' in window.component_status['model'].styleSheet()
assert not window.setup_button.isEnabled()
assert 'indexed files' in window.index_count.text()
assert '#c73545' in window.component_status['projector'].styleSheet()
with patch.object(window,'start_model') as start,patch.object(window,'refresh_component_status'),patch('desktop.catalogs') as catalogs:
    window.setup_all();start.assert_called_once();catalogs.assert_not_called()
window.settings['ffmpeg_path']=str(home/'missing-ffmpeg.exe');window.widgets['ffmpeg_path'].setText(window.settings['ffmpeg_path'])
window.setup_choices['ffmpeg'].setChecked(True)
window.launch=lambda work:work()
with patch('desktop.catalogs') as catalogs,patch('desktop.install_model') as model,patch('desktop.install_runtime') as runtime,patch('desktop.install_ocr') as ocr,patch('desktop.install_ffmpeg',return_value={'ffmpeg_path':str(home/'ffmpeg.exe')}) as decoder,patch.object(window,'start_model'),patch.object(window,'refresh_component_status'):
    window.setup_all()
    decoder.assert_called_once();catalogs.assert_not_called();model.assert_not_called();runtime.assert_not_called();ocr.assert_not_called()
window.navigation.setCurrentRow(2);QTest.qWait(30)
window.grab().save(str(home/'setup-options.png'))
window.cleanup();window.close()
print('Main and quick previews, text highlights, exact Explorer dispatch, status colors, Start button and selective setup passed.')
