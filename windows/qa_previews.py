import os
# Use the native Windows multimedia backend during this interaction check.
os.environ['VECTORDASH_SERVER_URL']='http://127.0.0.1:8788'
import hashlib,json,time,faulthandler
faulthandler.dump_traceback_later(20,repeat=True)
from pathlib import Path
from PySide6.QtWidgets import QApplication
from PySide6.QtCore import Qt
from PySide6.QtTest import QTest
from engine import Engine,DEFAULTS
from previews import Preview
from desktop import Results
app=QApplication([])
root=Path(__file__).parent.resolve()/'.smoke/multimedia'
managed=Path(__file__).parent.resolve()/'.smoke/managed'
info=json.loads((managed/'result.json').read_text())
settings=dict(DEFAULTS,semantic=True,model=info['model'],projector=str(managed/'models/mmproj-embeddinggemma-2-Q8_0.gguf'),code_enabled=True)
engine=Engine(root)
class Owner:
    def open_file(self,value): self.opened=value
owner=Owner()
results=Results(owner)
pdf=engine.search('the clause about pets in the lease',settings)[0]
original=hashlib.sha256(Path(pdf.path).read_bytes()).hexdigest()
results.populate([pdf])
assert results.selected_path()==pdf.path
assert results.selected_result().page==2
preview=Preview(pdf)
preview.show(); QTest.qWait(100)
assert preview.page.value()==2 and preview.highlight_count>=1
preview.grab().save(str(root/'pdf-preview.png'))
preview.close(); QTest.qWait(30)
assert hashlib.sha256(Path(pdf.path).read_bytes()).hexdigest()==original
image=engine.search('E_PAYMENT_402',settings)[0]
preview=Preview(image)
preview.show(); QTest.qWait(100)
assert not preview.picture.pixmap().isNull()
preview.grab().save(str(root/'image-preview.png'))
preview.close(); QTest.qWait(30)
code=engine.search('code: retry a failed call with backoff',settings)[0]
preview=Preview(code)
preview.show(); QTest.qWait(100)
assert preview.code_viewer.textCursor().blockNumber()==2
assert 'retry_request' in preview.code_viewer.toPlainText()
preview.grab().save(str(root/'code-preview.png'))
preview.close(); QTest.qWait(30)
for query in ('zebra in a video','where they talk about sleep'):
    result=engine.search(query,settings)[0]
    preview=Preview(result)
    preview.show()
    deadline=time.monotonic()+12
    while not hasattr(preview,'opened_position') and time.monotonic()<deadline:
        QTest.qWait(100)
    assert hasattr(preview,'opened_position'),preview.caption.text()
    assert abs(preview.opened_position-result.seconds*1000)<1
    preview.player.pause()
    assert preview.player.position()>=preview.opened_position-500,(query,preview.player.position(),preview.opened_position)
    print(query,'opened at',preview.opened_position,'ms',flush=True)
    print('Closing preview',flush=True)
    if result.kind=='audio':
        QTest.keyClick(preview,Qt.Key.Key_Escape)
    else:
        preview.close()
    QTest.qWait(100)
    assert preview.released
faulthandler.cancel_dump_traceback_later()
print('Native PDF highlights, image/code previews and real media seeking passed.',flush=True)
