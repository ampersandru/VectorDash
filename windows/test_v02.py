"""Security, hybrid retrieval, Office segmentation and MCP HTTP regressions."""
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import unittest
import urllib.request
import urllib.error
import zipfile
from unittest.mock import patch
from content import Segment
from engine import Engine,Settings,DEFAULTS
from mcp_server import McpServer
from office_content import office_segments,archive_segments
from security import MAGIC,contains_credentials

ROOT=Path(__file__).resolve().parents[1]
class UpgradeTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(dir=ROOT,prefix='vd-upgrade-');self.addCleanup(self.temp.cleanup)
        self.home=Path(self.temp.name);self.source=self.home/'source';self.source.mkdir()
        self.settings=dict(DEFAULTS,roots=[str(self.source)],ocr_enabled=False)
        self.engine=Engine(self.home)
    def test_individual_file_search_exclusion_persists_without_deleting(self):
        first=self.source/'first.txt';second=self.source/'second.txt'
        first.write_text('matching invoice INV-278');second.write_text('matching invoice INV-278')
        settings=dict(self.settings,semantic=False)
        self.engine.index(settings)
        self.assertEqual(len(self.engine.search('INV-278',settings)),2)
        store=Settings(self.home);store.values.update(settings)
        store.values['search_excluded_files']=[str(first)]
        store.save()
        loaded=Settings(self.home).values
        found=self.engine.search('INV-278',loaded)
        self.assertEqual([item.path for item in found],[str(second)])
        self.assertTrue(first.exists());self.assertEqual(self.engine.indexed_file_count(),2)
        server=McpServer(self.engine,lambda:loaded);server.active=True
        self.assertEqual(len(server.call('search_files',{'query':'INV-278'})['results']),1)
        loaded['search_excluded_files']=[]
        self.assertEqual(len(self.engine.search('INV-278',loaded)),2)

    def test_index_encrypted_and_reopens_without_plaintext(self):
        text='PRIVATE_SENTINEL_2764 invoice INV-2027'
        (self.source/'notes.txt').write_text(text)
        self.engine.index(self.settings)
        stored=self.engine.db.read_bytes()
        if os.name=='nt':
            self.assertFalse(stored.startswith(b'SQLite format 3'));self.assertNotIn(text.encode(),stored)
            self.assertFalse(Path(str(self.engine.db)+'-wal').exists())
        self.assertTrue(Engine(self.home).search('INV-2027',self.settings))
    def test_secrets_removed_from_regular_documents_and_ocr(self):
        (self.source/'secrets.txt').write_text('DO_NOT_INDEX_23842')
        (self.source/'ordinary.txt').write_text('api_key = sk-'+('abcdefg'*8))
        (self.source/'healthy.txt').write_text('healthy invoice INV-837')
        self.engine.index(self.settings)
        self.assertFalse(self.engine.search('DO_NOT_INDEX_23842',self.settings))
        self.assertFalse(self.engine.search('abcdefg',self.settings))
        self.assertTrue(self.engine.search('INV-837',self.settings))
        (self.source/'photo.png').write_bytes(b'fixture')
        with patch('engine.isolated_extract',return_value=iter([Segment(kind='image',text='password: VeryPrivatePassword123')])):
            self.engine.index(self.settings)
        self.assertFalse(self.engine.search('VeryPrivatePassword123',self.settings))
    def test_settings_tokens_protected_at_rest(self):
        store=Settings(self.home);store.values['remote_key']='SENSITIVE_TOKEN_13576';store.save()
        if os.name=='nt':self.assertNotIn('SENSITIVE_TOKEN_13576',store.path.read_text())
        self.assertEqual(Settings(self.home).values['remote_key'],'SENSITIVE_TOKEN_13576')
    def test_search_filters_and_reasons(self):
        (self.source/'invoice.txt').write_text('Invoice INV-2045 paid')
        (self.source/'invoice.log').write_text('Invoice INV-2045 declined')
        self.engine.index(self.settings)
        results=self.engine.search('INV-2045',dict(self.settings,search_excluded_types='.log'))
        self.assertEqual(len(results),1);self.assertTrue(results[0].reasons)
        self.assertIn('Contains the exact search phrase',results[0].reasons)
        self.assertEqual(self.engine.search('INV-2045',dict(self.settings,search_category='Photos / screenshots')),[])
    def test_excluded_folder_and_extension_not_indexed(self):
        nested=self.source/'personal';nested.mkdir();(nested/'note.txt').write_text('EXCLUDED_83765')
        (self.source/'logs.log').write_text('EXCLUDED_27464')
        self.engine.index(dict(self.settings,excluded_folders=[str(nested)],index_excluded_types='.log'))
        self.assertEqual(self.engine.indexed_file_count(),0)
    def test_legacy_credentials_purged_on_upgrade(self):
        legacy=self.home/'legacy';legacy.mkdir()
        with sqlite3.connect(legacy/'index.sqlite') as db:
            db.execute('CREATE TABLE files(path TEXT PRIMARY KEY,text TEXT,vector TEXT)')
            db.execute('INSERT INTO files VALUES(?,?,?)',(str(self.source/'note.txt'),'password: TopSecret98765','[]'))
        db.close()
        self.assertEqual(Engine(legacy).indexed_file_count(),0)
    def test_xlsx_preserves_shared_strings_and_cell_locations(self):
        path=self.source/'budget.xlsx'
        with zipfile.ZipFile(path,'w') as z:
            z.writestr('xl/workbook.xml','<workbook xmlns="x"><sheets><sheet name="Budget"/></sheets></workbook>')
            z.writestr('xl/sharedStrings.xml','<sst xmlns="x"><si><t>Invoice INV-45678</t></si></sst>')
            z.writestr('xl/worksheets/sheet1.xml','<worksheet xmlns="x"><sheetData><row><c r="A1" t="s"><v>0</v></c><c r="B1"><v>1250</v></c></row></sheetData></worksheet>')
        self.engine.index(self.settings)
        result=self.engine.search('INV-45678',self.settings)[0]
        self.assertEqual(result.symbol,'Budget');self.assertIn('A1',result.snippet)
    def test_xlsx_uses_relationships_for_reordered_worksheets(self):
        path=self.source/'reordered.xlsx'
        with zipfile.ZipFile(path,'w') as z:
            z.writestr('xl/workbook.xml','<workbook xmlns="x" xmlns:r="rel"><sheets><sheet name="First" r:id="rId3"/><sheet name="Second" r:id="rId1"/></sheets></workbook>')
            z.writestr('xl/_rels/workbook.xml.rels','<Relationships><Relationship Id="rId3" Target="worksheets/sheet3.xml"/><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>')
            z.writestr('xl/worksheets/sheet3.xml','<worksheet xmlns="x"><c r="A1" t="inlineStr"><is><t>FIRST_SENTINEL</t></is></c></worksheet>')
            z.writestr('xl/worksheets/sheet1.xml','<worksheet xmlns="x"><c r="A1" t="inlineStr"><is><t>SECOND_SENTINEL</t></is></c></worksheet>')
        segments=list(office_segments(path))
        self.assertEqual(segments[0].symbol,'First');self.assertIn('FIRST_SENTINEL',segments[0].text)
        self.assertEqual(segments[1].symbol,'Second');self.assertIn('SECOND_SENTINEL',segments[1].text)

    def test_pptx_has_slide_location(self):
        path=self.source/'notes.pptx'
        with zipfile.ZipFile(path,'w') as z:
            z.writestr('ppt/slides/slide1.xml','<slide xmlns="x"><t>Sleep regulation and insomnia</t></slide>')
            z.writestr('ppt/slides/slide2.xml','<slide xmlns="x"><t>Payment error ERR-987</t></slide>')
        self.engine.index(self.settings)
        result=self.engine.search('ERR-987',self.settings)[0];self.assertEqual(result.symbol,'Slide 2')
    def test_zip_manifest_excludes_traversal_and_never_extracts(self):
        path=self.source/'bundle.zip'
        with zipfile.ZipFile(path,'w') as z:
            z.writestr('../escape.txt','PRIVATE');z.writestr('report.txt','PRIVATE');z.writestr('secrets.txt','PRIVATE')
        text=' '.join(x.text for x in archive_segments(path))
        self.assertIn('report.txt',text);self.assertNotIn('escape',text);self.assertNotIn('secrets',text);self.assertNotIn('PRIVATE',text)
    def test_office_zip_bomb_is_rejected(self):
        path=self.source/'bomb.docx'
        with zipfile.ZipFile(path,'w',compression=zipfile.ZIP_DEFLATED) as z:z.writestr('word/document.xml','a'*1000000)
        with self.assertRaisesRegex(ValueError,'decompression'):list(office_segments(path))
    def test_mismatched_embedding_dimensions_rejected(self):
        (self.source/'note.txt').write_text('beach dog')
        self.engine.index(self.settings)
        with self.engine.connect() as db:
            db.execute("UPDATE segments SET vector='[1,0]'")
            db.execute("INSERT OR REPLACE INTO metadata VALUES('signature','fixture')")
        with patch('engine.verify_server_model'),patch('engine.model_signature',return_value='fixture'),patch('engine.embed',return_value=[1,0,0]):
            with self.assertRaisesRegex(ValueError,'dimensions'):self.engine.search('dog',dict(self.settings,semantic=True))
    @unittest.skipUnless(os.name=='nt','Windows job objects')
    def test_parser_exit_kills_owned_helpers(self):
        import subprocess
        import sys
        import psutil
        code="import os,sys,subprocess;sys.path.insert(0,"+repr(str(ROOT/'windows'))+");from parser_worker import constrain_worker;constrain_worker();child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);print(child.pid,flush=True);os._exit(0)"
        result=subprocess.run([sys.executable,'-c',code],capture_output=True,text=True,timeout=10,creationflags=0x08000000,check=True)
        pid=int(result.stdout.strip())
        try:
            child=psutil.Process(pid)
            child.wait(timeout=3)
        except psutil.NoSuchProcess:pass
        self.assertFalse(psutil.pid_exists(pid))

    def test_downloads_without_publisher_checksums_refused(self):
        import threading
        from setup_runtime import install_model,install_runtime
        with self.assertRaisesRegex(RuntimeError,'SHA-256'):
            install_model({'name':'test.gguf','size':1},self.home,threading.Event(),lambda *args:None)
        with self.assertRaisesRegex(RuntimeError,'SHA-256'):
            install_runtime({'version':'test','backend':'cpu','assets':[{'name':'server.zip','size':1}]},self.home,threading.Event(),lambda *args:None)

    def test_mcp_auth_origin_and_readonly_scope(self):
        (self.source/'note.txt').write_text('Invoice INV-900')
        self.engine.index(self.settings)
        token='a_secure_test_token_1234567890'
        server=McpServer(self.engine,lambda:dict(self.settings,mcp_port=0,mcp_auth=True,mcp_token=token));server.start();self.addCleanup(server.stop)
        endpoint=f'http://127.0.0.1:{server.port}/mcp'
        def request(method,params=None,headers=None):
            data=json.dumps({'jsonrpc':'2.0','id':1,'method':method,'params':params or {}}).encode()
            with urllib.request.urlopen(urllib.request.Request(endpoint,data,headers={'Content-Type':'application/json',**(headers or {})}),timeout=5) as response:return json.load(response)
        auth={'Authorization':'Bearer '+token}
        with self.assertRaises(urllib.error.HTTPError) as error:request('tools/list')
        self.assertEqual(error.exception.code,401);error.exception.close()
        with self.assertRaises(urllib.error.HTTPError) as error:request('tools/list',headers=dict(auth,Origin='https://evil.example'))
        self.assertEqual(error.exception.code,403);error.exception.close()
        info=request('initialize',{'protocolVersion':'2025-11-25'},auth)
        self.assertEqual(info['result']['protocolVersion'],'2025-11-25')
        result=request('tools/call',{'name':'search_files','arguments':{'query':'INV-900'}},auth)
        self.assertFalse(result['result']['isError']);self.assertIn('INV-900',result['result']['content'][0]['text'])
        blocked=request('tools/call',{'name':'file_info','arguments':{'path':str(ROOT/'README.md')}},auth)
        self.assertTrue(blocked['result']['isError'])
        self.assertTrue(request('tools/call',{'name':'delete_file','arguments':{}},auth)['result']['isError'])
        with self.assertRaises(urllib.error.URLError):urllib.request.urlopen('http://127.0.0.1:1/mcp',timeout=.1)
        server.stop()
        with self.assertRaises(urllib.error.URLError):urllib.request.urlopen(endpoint,timeout=.1)

if __name__=='__main__':unittest.main()
