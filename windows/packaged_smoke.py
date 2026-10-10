"""Opt-in smoke check for packaged Windows dependencies, UI and local indexing."""
import json
import os
from pathlib import Path
import sys
import time
import traceback

def run_smoke(app):
    report=Path(sys.argv[sys.argv.index('--smoke-report')+1])
    report.parent.mkdir(parents=True,exist_ok=True)
    result={'ok':False,'frozen':bool(getattr(sys,'frozen',False))}
    window=None
    try:
        from PIL import Image
        import numpy
        import pymupdf
        from engine import Settings,DEFAULTS,SearchResult,Engine
        from desktop import Desktop
        from content import ffmpeg
        from windows_integration import startup_command
        import process_lifecycle
        home=report.parent/'app-data'
        os.environ['VECTORDASH_DATA_HOME']=str(home)
        store=Settings(home)
        store.values=dict(DEFAULTS,hotkey='Ctrl+Alt+Shift+F19',theme='Light',ocr_enabled=False)
        window=Desktop(store)
        window.clock.stop();window.process_timer.stop()
        source=home/'source';source.mkdir(parents=True,exist_ok=True)
        photo=source/'preview.png';Image.new('RGB',(120,80),'purple').save(photo)
        (source/'notes.txt').write_text('Packaged search invoice INV-394',encoding='utf-8')
        pdf=source/'preview.pdf'
        with pymupdf.open() as doc:
            page=doc.new_page();page.insert_text((40,40),'Pets are allowed with permission.')
            doc.save(pdf)
        settings=dict(store.values,semantic=False,roots=[str(source)])
        summary=window.engine.index(settings)
        assert window.engine.search('INV-394',settings)
        window.results.populate([SearchResult(1,str(photo),'',kind='image')])
        app.processEvents()
        assert not window.result_preview.current.picture.pixmap().isNull()
        window.quick.results.populate([SearchResult(1,str(pdf),'Pets',kind='pdf',page=1,query='Pets')])
        app.processEvents()
        assert window.quick.preview.current.highlight_count>0
        import imageio_ffmpeg
        decoder=imageio_ffmpeg.get_ffmpeg_exe()
        checked=process_lifecycle.run([decoder,'-version'],capture_output=True,timeout=20,creationflags=0x08000000)
        assert checked.returncode==0
        if getattr(sys,'frozen',False):
            assert startup_command()==[sys.executable,'--startup']
        result.update(ok=True,index=summary,ffmpeg=Path(decoder).name,numpy=numpy.__version__,startup=startup_command())
        window.grab().save(str(report.with_suffix('.png')))
    except Exception:
        result['error']=traceback.format_exc()
    finally:
        if window:
            window.cleanup();window.close()
        report.write_text(json.dumps(result,indent=2),encoding='utf-8')
    return 0 if result['ok'] else 1
