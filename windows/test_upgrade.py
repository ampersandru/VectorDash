"""Regression checks for the Windows desktop upgrade."""
from datetime import datetime
import hashlib
import io
import json
from pathlib import Path
import sqlite3
import tempfile
import threading
import unittest
from unittest.mock import patch
import zipfile
from engine import DEFAULTS, Engine, Settings, verify_server_model
from scheduling import schedule_due, idle_allowed, next_run
from windows_integration import hotkey_parts, parse_devices
from setup_runtime import download, extract_archive, catalogs

WORKSPACE=Path(__file__).resolve().parents[1]

def temporary():
    return tempfile.TemporaryDirectory(prefix='digup-test-',dir=WORKSPACE)

class IndexTests(unittest.TestCase):
    def setUp(self):
        self.temp=temporary()
        self.addCleanup(self.temp.cleanup)
        self.home=Path(self.temp.name)
        self.source=self.home/'source'
        self.source.mkdir()
        self.file=self.source/'notes.txt'
        self.file.write_text('Windows search with a remembered phrase',encoding='utf-8')
        self.settings=dict(DEFAULTS,roots=[str(self.source)])
        self.engine=Engine(self.home)

    def test_incremental_reuse_and_deleted_files(self):
        self.engine.index(self.settings)
        with patch('engine.read_text',side_effect=AssertionError('unchanged file re-read')):
            result=self.engine.index(self.settings)
        self.assertIn('1 reused',result)
        self.assertEqual(len(self.engine.search('remembered',self.settings)),1)
        self.file.unlink()
        self.engine.index(self.settings)
        self.assertEqual(self.engine.search('remembered',self.settings),[])

    def test_changed_file_is_reindexed(self):
        self.engine.index(self.settings)
        self.file.write_text('A different, much longer text describing solar panels',encoding='utf-8')
        self.engine.index(self.settings)
        self.assertEqual(len(self.engine.search('solar',self.settings)),1)
        self.assertEqual(self.engine.search('remembered',self.settings),[])

    def test_cancel_retains_previous_index(self):
        self.engine.index(self.settings)
        self.engine.cancel.set()
        with self.assertRaises(InterruptedError):
            self.engine.index(self.settings)
        self.assertEqual(len(self.engine.search('remembered',self.settings)),1)

    def test_missing_drive_retains_previous_index(self):
        self.engine.index(self.settings)
        settings=dict(self.settings,roots=[str(self.home/'disconnected')])
        with self.assertRaises(ValueError):
            self.engine.index(settings)
        self.assertEqual(len(self.engine.search('remembered',self.settings)),1)

    def test_migrates_original_schema(self):
        old=self.home/'legacy'
        old.mkdir()
        with sqlite3.connect(old/'index.sqlite') as db:
            db.execute('CREATE TABLE files(path TEXT PRIMARY KEY,text TEXT,vector TEXT)')
            db.execute('INSERT INTO files VALUES(?,?,?)',(str(self.file),'legacy notes','[]'))
        db.close()
        engine=Engine(old)
        self.assertEqual(len(engine.search('legacy',self.settings)),1)
        with engine.connect() as db:
            self.assertTrue({'mtime','size'}.issubset({row[1] for row in db.execute('PRAGMA table_info(files)')}))

    def test_search_while_index_paused(self):
        self.engine.index(self.settings)
        self.engine.paused.set()
        done=threading.Event()
        def work():
            try:
                self.engine.index(self.settings)
            except InterruptedError:
                pass
            finally:
                done.set()
        thread=threading.Thread(target=work)
        thread.start()
        try:
            self.assertEqual(len(self.engine.search('remembered',self.settings)),1)
        finally:
            self.engine.cancel.set()
            self.assertTrue(done.wait(3))
            thread.join()

    def test_atomic_settings_preserve_existing_paths(self):
        (self.home/'settings.json').write_text(json.dumps({'server':'H:/llama/server.exe','model':'C:/models/model.gguf','device':'CUDA0'}))
        settings=Settings(self.home)
        settings.save()
        self.assertEqual(Settings(self.home).values['device'],'CUDA0')
        self.assertEqual(Settings(self.home).values['server'],'H:/llama/server.exe')

class ScheduleTests(unittest.TestCase):
    def setUp(self):
        self.settings=dict(DEFAULTS,schedule_mode='At selected time',schedule_enabled_at='2026-10-10T01:00:00')
        self.metrics={'cpu':10,'gpu':5,'idle':600,'on_ac':True}

    def test_daily_run_only_once(self):
        now=datetime(2026,10,10,3)
        due,slot,_=schedule_due(self.settings,now,self.metrics)
        self.assertTrue(due)
        self.settings['last_schedule_slot']=slot
        self.assertFalse(schedule_due(self.settings,now,self.metrics)[0])

    def test_no_immediate_historical_run_on_enabling(self):
        self.settings['schedule_enabled_at']='2026-10-10T12:00:00'
        self.assertFalse(schedule_due(self.settings,datetime(2026,10,10,12,1),self.metrics)[0])

    def test_sleep_resume_catches_one_latest_slot(self):
        self.settings['last_schedule_slot']='2026-10-10T02:00:00'
        due,slot,_=schedule_due(self.settings,datetime(2026,10,13,12),self.metrics)
        self.assertTrue(due)
        self.assertEqual(slot,'2026-10-13T02:00:00')

    def test_cpu_gpu_and_user_idle_conditions(self):
        self.settings.update(schedule_mode='When idle',check_gpu=True)
        self.assertTrue(schedule_due(self.settings,datetime(2026,10,10,3),self.metrics)[0])
        for key,value in [('cpu',90),('gpu',90),('idle',0),('on_ac',False),('gpu',None)]:
            self.assertFalse(idle_allowed(self.settings,dict(self.metrics,**{key:value}))[0])

    def test_idle_repeat_interval(self):
        self.settings.update(schedule_mode='When idle',last_success='2026-10-10T02:30:00')
        self.assertFalse(schedule_due(self.settings,datetime(2026,10,10,3),self.metrics)[0])

    def test_time_plus_idle(self):
        self.settings['schedule_mode']='Time + idle'
        self.assertFalse(schedule_due(self.settings,datetime(2026,10,10,1,30),self.metrics)[0])
        self.assertFalse(schedule_due(self.settings,datetime(2026,10,10,3),dict(self.metrics,idle=0))[0])
        self.assertTrue(schedule_due(self.settings,datetime(2026,10,10,3),self.metrics)[0])

    def test_resources_only_idle_mode(self):
        self.settings.update(schedule_mode='When idle',check_user_idle=False)
        self.assertTrue(schedule_due(self.settings,datetime(2026,10,10,3),dict(self.metrics,idle=0))[0])

    def test_weekday_selection(self):
        self.settings['schedule_days']=[0]
        self.assertIn('Mon',next_run(self.settings,datetime(2026,10,10,3)))

class IntegrationTests(unittest.TestCase):
    def test_hotkey_validation(self):
        self.assertEqual(hotkey_parts('Ctrl+Shift+Space'),(0x4006,0x20))
        for invalid in ('Space','Ctrl+Unknown','Ctrl+Ctrl++','Ctrl+F25'):
            with self.assertRaises(ValueError):
                hotkey_parts(invalid)

    def test_running_model_must_match_selection(self):
        class Response(io.BytesIO):
            pass
        with patch('engine.urllib.request.urlopen',return_value=Response(b'{"model_path":"C:/models/wrong.gguf"}')):
            with self.assertRaisesRegex(ValueError,'different model'):
                verify_server_model(dict(DEFAULTS,semantic=True,model='C:/models/chosen.gguf'))

    def test_device_selection_preserves_identifiers(self):
        devices=parse_devices('ggml: init\nAvailable devices:\n  CUDA0: NVIDIA RTX (16000 MiB)\n  Vulkan1: Intel UHD (8000 MiB)\n')
        self.assertEqual([item[0] for item in devices],['CUDA0','Vulkan1'])

    def test_archive_traversal_blocked(self):
        with temporary() as directory:
            root=Path(directory)
            archive=root/'bad.zip'
            with zipfile.ZipFile(archive,'w') as bundle:
                bundle.writestr('../escape.exe','bad')
            with self.assertRaises(RuntimeError):
                extract_archive(archive,root/'install')
            self.assertFalse((root/'escape.exe').exists())

    def test_download_checksum_and_cancel(self):
        class Response(io.BytesIO):
            status=200
            headers={}
        data=b'verified model bytes'
        with temporary() as directory:
            target=Path(directory)/'model.gguf'
            with patch('setup_runtime.urllib.request.urlopen',return_value=Response(data)):
                download('https://example.invalid/model',target,len(data),hashlib.sha256(data).hexdigest(),threading.Event(),lambda *args:None)
            self.assertEqual(target.read_bytes(),data)
            cancel=threading.Event()
            cancel.set()
            with patch('setup_runtime.urllib.request.urlopen',return_value=Response(data)):
                with self.assertRaises(InterruptedError):
                    download('https://example.invalid/model',target,len(data),None,cancel,lambda *args:None)

    def test_corrupt_download_not_installed(self):
        class Response(io.BytesIO):
            status=200
            headers={}
        with temporary() as directory:
            target=Path(directory)/'model.gguf'
            with patch('setup_runtime.urllib.request.urlopen',return_value=Response(b'wrong')):
                with self.assertRaisesRegex(RuntimeError,'Checksum mismatch'):
                    download('https://example.invalid/model',target,5,'0'*64,threading.Event(),lambda *args:None)
            self.assertFalse(target.exists())
            self.assertFalse(target.with_name(target.name+'.part').exists())

    def test_download_resume(self):
        class Response(io.BytesIO):
            status=206
            headers={'Content-Range':'bytes 3-5/6'}
        with temporary() as directory:
            target=Path(directory)/'model.gguf'
            target.with_name(target.name+'.part').write_bytes(b'abc')
            with patch('setup_runtime.urllib.request.urlopen',return_value=Response(b'def')) as request:
                download('https://example.invalid/model',target,6,hashlib.sha256(b'abcdef').hexdigest(),threading.Event(),lambda *args:None)
                self.assertEqual(request.call_args.args[0].headers['Range'],'bytes=3-')
            self.assertEqual(target.read_bytes(),b'abcdef')

if __name__=='__main__':
    unittest.main()
