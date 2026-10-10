"""Remote endpoint, migration and failed-image behavior regressions."""
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from PIL import Image
from engine import Engine,DEFAULTS,Settings,data_home,model_signature,verify_server_model
from vectordash_core import embedding_request,server_url
import process_lifecycle

class Response(io.BytesIO):
    def __init__(self,data):
        super().__init__(json.dumps(data).encode())

class FeatureTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home=Path(self.temp.name)
        process_lifecycle.reopen()
        self.addCleanup(process_lifecycle.reopen)

    def test_remote_text_and_media_use_endpoint_and_auth(self):
        settings=dict(DEFAULTS,server_mode='Remote',remote_url='https://example.invalid:8787/',remote_key='test-key')
        calls=[]
        def request(req,**kwargs):
            calls.append(req)
            return Response({'media_marker':'<media>'}) if req.full_url.endswith('/props') else Response({'data':[{'embedding':[1,2,3]}]})
        with patch('vectordash_core.urllib.request.urlopen',side_effect=request):
            self.assertEqual(embedding_request('hello',settings),[1,2,3])
            self.assertEqual(embedding_request(b'image',settings),[1,2,3])
        self.assertEqual(len(calls),3)
        for req in calls:
            self.assertTrue(req.full_url.startswith('https://example.invalid:8787/'))
            self.assertEqual(req.headers['Authorization'],'Bearer test-key')
        media=json.loads(calls[-1].data)['input']
        self.assertEqual(media['multimodal_data'],['aW1hZ2U='])

    def test_remote_signature_requires_no_local_model_and_checks_identity(self):
        settings=dict(DEFAULTS,semantic=True,server_mode='Remote',remote_url='http://remote:8787')
        self.assertIn('remote',model_signature(settings))
        with patch('engine.urllib.request.urlopen',return_value=Response({'model_path':'/models/embeddinggemma-2-Q8_0.gguf'})):
            verify_server_model(settings)
        with patch('engine.urllib.request.urlopen',return_value=Response({'model_path':'/models/unrelated.gguf'})):
            with self.assertRaises(ValueError):verify_server_model(settings)

    def test_legacy_folder_migrates_with_download_paths_and_index(self):
        legacy=self.home/'DigUp';legacy.mkdir()
        (legacy/'models').mkdir()
        model=legacy/'models/model.gguf';model.write_bytes(b'model')
        (legacy/'settings.json').write_text(json.dumps({'model':str(model),'roots':['C:/Documents']}))
        (legacy/'index.sqlite').write_bytes(b'index')
        with patch.dict(os.environ,{'LOCALAPPDATA':str(self.home)},clear=True):
            target=data_home()
            self.assertEqual(target,self.home/'VectorDash')
            values=Settings().values
        self.assertFalse(legacy.exists())
        self.assertTrue(Path(values['model']).is_file())
        self.assertEqual((target/'index.sqlite').read_bytes(),b'index')
        self.assertEqual(values['roots'],['C:/Documents'])

    def test_large_image_is_skipped_without_losing_other_files(self):
        source=self.home/'source';source.mkdir()
        Image.new('RGB',(20,20),'white').save(source/'huge.png')
        (source/'good.txt').write_text('healthy invoice INV-99')
        settings=dict(DEFAULTS,roots=[str(source)],ocr_enabled=False)
        engine=Engine(self.home)
        with patch.object(Image,'MAX_IMAGE_PIXELS',100),patch('engine.isolated_extract',side_effect=lambda path,settings,checkpoint:__import__('content').extract(path,settings,__import__('vectordash_core').read_text,checkpoint)),self.assertLogs('vectordash',level='WARNING'):
            result=engine.index(settings)
        self.assertIn('1 skipped',result)
        self.assertTrue(engine.search('INV-99',settings))
        self.assertTrue(engine.search('huge',settings))

    def test_remote_index_and_search_over_http(self):
        from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
        import threading
        calls=[]
        class Handler(BaseHTTPRequestHandler):
            def log_message(self,*args):pass
            def do_GET(self):
                calls.append((self.path,self.headers.get('Authorization')))
                self.send_response(200);self.end_headers()
                self.wfile.write(json.dumps({'model_path':'/remote/embeddinggemma-2-Q8_0.gguf','modalities':{}}).encode())
            def do_POST(self):
                calls.append((self.path,self.headers.get('Authorization')))
                json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                self.send_response(200);self.end_headers()
                self.wfile.write(json.dumps({'data':[{'embedding':[1.0,0.0,0.0]}]}).encode())
        server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
        worker=threading.Thread(target=server.serve_forever,daemon=True);worker.start()
        try:
            source=self.home/'source';source.mkdir()
            (source/'remote.txt').write_text('remote transport invoice INV-43')
            settings=dict(DEFAULTS,semantic=True,roots=[str(source)],ocr_enabled=False,server_mode='Remote',remote_url='http://127.0.0.1:'+str(server.server_port),remote_key='secret')
            engine=Engine(self.home)
            self.assertIn('1 files indexed',engine.index(settings))
            self.assertTrue(engine.search('INV-43',settings))
            self.assertTrue(any(path=='/v1/embeddings' for path,key in calls))
            self.assertTrue(all(key=='Bearer secret' for path,key in calls))
        finally:
            server.shutdown();server.server_close();worker.join()

    def test_picture_query_ranks_subject_above_generic_ocr_words(self):
        settings=dict(DEFAULTS,semantic=True,model=str(self.home/'model.gguf'))
        Path(settings['model']).write_bytes(b'model')
        engine=Engine(self.home)
        with engine.connect() as db:
            db.execute("INSERT OR REPLACE INTO metadata VALUES('signature',?)",(model_signature(settings),))
            for name,text,vector in [('compatibility.png','Image upload: pasted images stay inline',[.2,.98]),('dogs.jpg','',[1,0]),('error.png','Payment declined E_PAYMENT_402',[.3,.95])]:
                db.execute('INSERT INTO segments VALUES(?,?,?,?,?,?)',(str(self.home/name),'files','image',text,json.dumps(vector),'{}'))
            db.execute('INSERT INTO words(rowid,path,text) SELECT rowid,path,text FROM segments')
        with patch('engine.verify_server_model'),patch('engine.embed',return_value=[1,0]) as embed:
            images=engine.search('images of dogs',settings)
            self.assertEqual(Path(images[0].path).name,'dogs.jpg')
            self.assertFalse(any(Path(result.path).name=='compatibility.png' for result in images))
            self.assertIn('query: dogs',embed.call_args.args[0])
            exact=engine.search('E_PAYMENT_402',settings)
            self.assertEqual(Path(exact[0].path).name,'error.png')

    def test_query_embedding_is_reused_and_index_cache_invalidates(self):
        settings=dict(DEFAULTS,semantic=True,model=str(self.home/'model.gguf'))
        Path(settings['model']).write_bytes(b'model')
        engine=Engine(self.home)
        with engine.connect() as db:
            db.execute("INSERT INTO metadata VALUES('signature',?)",(model_signature(settings),))
            db.execute('INSERT INTO segments VALUES(?,?,?,?,?,?)',('first.txt','files','text','first','[1,0]','{}'))
            db.execute('INSERT INTO words(rowid,path,text) SELECT rowid,path,text FROM segments')
        with patch('engine.verify_server_model'),patch('engine.embed',return_value=[1,0]) as embed:
            self.assertEqual(engine.search('test subject',settings)[0].path,'first.txt')
            engine.search('test subject',settings)
            embed.assert_called_once()
            with engine.connect() as db:
                db.execute('DELETE FROM segments')
                db.execute('INSERT INTO segments VALUES(?,?,?,?,?,?)',('second.txt','files','text','second','[1,0]','{}'))
                db.execute('DELETE FROM words')
                db.execute('INSERT INTO words(rowid,path,text) SELECT rowid,path,text FROM segments')
            self.assertEqual(engine.search('test subject',settings)[0].path,'second.txt')
            engine.clear_query_cache()
            engine.search('test subject',settings)
            self.assertEqual(embed.call_count,2)
        self.assertEqual(engine.indexed_file_count(),1)

    def test_shutdown_blocks_new_helpers(self):
        process_lifecycle.close()
        with patch('process_lifecycle.subprocess.Popen') as spawn:
            with self.assertRaises(InterruptedError):process_lifecycle.popen(['anything'])
            spawn.assert_not_called()

if __name__=='__main__':unittest.main()
