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
        store.values=dict(DEFAULTS,mcp_token='smoke_token_12345678901234567890',hotkey='Ctrl+Alt+Shift+F19',theme='Light',ocr_enabled=False)
        window=Desktop(store)
        program_count,program_warnings=window.engine.programs.scan()
        assert program_count>0 and not program_warnings,program_warnings
        program_settings=dict(store.values,programs_enabled=True,search_category='Programs')
        calculator=window.engine.search('app: calculator',program_settings)
        assert calculator and calculator[0].kind=='application'
        from program_icons import load_icon
        program_image=load_icon(calculator[0].path)
        assert program_image is not None and not program_image.isNull()
        window.request_search('24*1.2')
        assert window.results.selected_result().symbol=='28.8'
        assert window.quick.results.selected_result().kind=='calculator'
        from PySide6.QtWidgets import QApplication
        from PySide6.QtCore import QMimeData
        previous=QApplication.clipboard().mimeData();saved_clipboard=QMimeData()
        if previous:
            for format_name in previous.formats():saved_clipboard.setData(format_name,previous.data(format_name))
        try:
            window.open_file(window.results.selected_result())
            assert QApplication.clipboard().text()=='28.8'
        finally:QApplication.clipboard().setMimeData(saved_clipboard)
        window.request_search('settings: bluetooth')
        assert window.results.selected_result().path=='ms-settings:bluetooth'
        window.quick.input.clear();window.quick.timer.stop()
        assert window.quick.split.isHidden() and window.quick.height()<230
        window.quick.input.setText('report');window.quick.timer.stop()
        assert not window.quick.split.isHidden() and window.quick.height()==570
        window.quick.input.clear();window.quick.timer.stop()
        result['compact_popup']=True
        result['quick_actions']=True
        result['program_icons']=True
        result['programs']=program_count
        window.settings['semantic']=False
        window.clock.stop();window.process_timer.stop()
        source=home/'source';source.mkdir(parents=True,exist_ok=True)
        photo=source/'preview.png';Image.new('RGB',(120,80),'purple').save(photo)
        (source/'notes.txt').write_text('Packaged search invoice INV-394',encoding='utf-8')
        pdf=source/'preview.pdf'
        pdf.unlink(missing_ok=True)
        with pymupdf.open() as doc:
            page=doc.new_page();page.insert_text((40,40),'Pets are allowed with permission.')
            doc.save(pdf)
        settings=dict(store.values,semantic=False,roots=[str(source)])
        summary=window.engine.index(settings)
        version=window.engine.storage_version
        window.engine.index(settings)
        assert window.engine.storage_version==version
        assert not hasattr(window.engine,'snapshot')
        assert 'exclude_windows_churn' in window.widgets
        assert 'performance_profile' in window.widgets
        from name_index import normalized
        stats=window.engine.source_stats(settings)
        assert stats[normalized(source)]=={'files':3,'added_hour':3},stats
        for selected_profile in ('Low memory','Balanced','Fast search'):
            assert window.engine.search('notes',dict(settings,search_mode='Names',performance_profile=selected_profile))
        result['resource_profiles']=True
        result['source_activity']=True
        result['incremental_memory']=True
        assert window.engine.search('INV-394',settings)
        folder_result=SearchResult(2,str(source),'',kind='folder')
        for results in (window.results,window.quick.results):
            results.populate([folder_result,SearchResult(1,str(photo),'',kind='image')])
            assert results.count()==2 and results.selected_result().kind=='folder'
        result['folder_results']=True
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
        # Names search and quick indexing must work with a deliberately missing model.
        from name_index import normalized
        quick_settings=dict(settings,semantic=True,model='missing-model.gguf',source_modes={normalized(source):'names'},search_mode='Names')
        window.engine.index(quick_settings)
        assert window.engine.search('notes',quick_settings)[0].path==str(source/'notes.txt')
        folders=window.engine.search('source',dict(quick_settings,search_category='Folders'))
        assert folders and folders[0].kind=='folder'
        assert not window.engine.search('notes',dict(quick_settings,search_sources=[]))
        window.engine.index(settings)
        with window.engine.connect(readonly=True) as disk:
            assert disk.execute('SELECT count(*) FROM files').fetchone()[0]==3
            assert disk.execute('PRAGMA database_list').fetchone()[2]
            assert disk.execute('PRAGMA cache_size').fetchone()[0]==-8192
        assert not window.engine.db.read_bytes().startswith(b'SQLite format 3')
        assert 'encrypt_index' in window.widgets
        assert 'INV-394' not in window.engine.db.read_bytes().decode('latin1')
        # Validate the actual packaged APSW VFS and SQLCipher native modules,
        # not only fresh-database indexing.
        import sqlite3
        from security import seal_to_file
        legacy_home=home/'legacy-storage';legacy_home.mkdir(exist_ok=True)
        legacy=sqlite3.connect(':memory:')
        legacy.execute('CREATE TABLE files(path TEXT PRIMARY KEY,text TEXT,vector TEXT)')
        legacy.execute('INSERT INTO files VALUES(?,?,?)',(str(source/'notes.txt'),'Legacy invoice INV-LEGACY-77','[]'))
        legacy.execute('CREATE TABLE padding(data BLOB)')
        legacy.execute('INSERT INTO padding VALUES(zeroblob(9000000))');legacy.commit()
        seal_to_file(legacy.serialize(),legacy_home/'index.sqlite');legacy.close()
        upgraded=Engine(legacy_home,dict(DEFAULTS,exclude_sensitive=False))
        assert upgraded.search('INV-LEGACY-77',dict(DEFAULTS,semantic=False))
        unencrypted=Engine(legacy_home,dict(DEFAULTS,encrypt_index=False,exclude_sensitive=False))
        assert unencrypted.db.read_bytes().startswith(b'SQLite format 3')
        assert unencrypted.search('INV-LEGACY-77',dict(DEFAULTS,semantic=False))
        encrypted=Engine(legacy_home,dict(DEFAULTS,encrypt_index=True,exclude_sensitive=False))
        assert not encrypted.db.read_bytes().startswith(b'SQLite format 3')
        assert encrypted.search('INV-LEGACY-77',dict(DEFAULTS,semantic=False))
        result['disk_storage_upgrade']=True
        result['encryption_toggle']=True
        found=window.engine.search('INV-394',settings)[0]
        assert found.reasons and found.keyword_score>0
        window.quick.show();app.processEvents()
        window.preview_result(SearchResult(.7,str(photo),'',kind='image',reasons=['Matches the picture']))
        app.processEvents()
        assert window.quick.isVisible() and window.large_preview.isVisible()
        window.large_preview.close();app.processEvents()
        assert window.quick.isVisible() and not window.quick.preview_open
        from mcp_server import McpServer
        import urllib.request
        server=McpServer(window.engine,lambda:dict(settings,mcp_port=0,mcp_auth=True,mcp_token=store.values['mcp_token']))
        server.start()
        try:
            body=json.dumps({'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'name':'search_files','arguments':{'query':'INV-394'}}}).encode()
            request=urllib.request.Request(f'http://127.0.0.1:{server.port}/mcp',body,headers={'Content-Type':'application/json','Authorization':'Bearer '+store.values['mcp_token']})
            with urllib.request.urlopen(request,timeout=5) as response: assert not json.load(response)['result']['isError']
        finally:server.stop()
        result.update(ok=True,names_only=True,protected_index=True,mcp=True,preview_return=True,index=summary,ffmpeg=Path(decoder).name,numpy=numpy.__version__,startup=startup_command())
        window.grab().save(str(report.with_suffix('.png')))
    except Exception:
        result['error']=traceback.format_exc()
    finally:
        if window:
            window.cleanup();window.close()
        report.write_text(json.dumps(result,indent=2),encoding='utf-8')
    return 0 if result['ok'] else 1
