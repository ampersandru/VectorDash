"""Content retrieval, exact identifiers, atomic index and project isolation checks."""
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from PIL import Image,ImageDraw,ImageFont
import pymupdf
from content import Segment,extract,code_segments,project_files,ocr_program,chunks
from engine import Engine,DEFAULTS,model_signature
from vectordash_core import read_text,embedding_request,command

ROOT=Path(__file__).resolve().parents[1]

class ContentTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(dir=ROOT,prefix='vectordash-content-')
        self.addCleanup(self.temp.cleanup)
        self.home=Path(self.temp.name)
        self.source=self.home/'source'; self.source.mkdir()
        self.engine=Engine(self.home)
        self.settings=dict(DEFAULTS,roots=[str(self.source)],ocr_enabled=False)

    def pdf(self):
        path=self.source/'agreement.pdf'
        with pymupdf.open() as document:
            page=document.new_page(); page.insert_text((72,72),'Rent payment terms and the landlord address.')
            page=document.new_page(); page.insert_text((72,72),'Pets clause: dogs require written approval. Invoice INV-77891.')
            document.save(path)
        return path

    def test_pdf_result_has_matching_page_and_exact_identifier(self):
        self.pdf(); self.engine.index(self.settings)
        result=self.engine.search('INV-77891',self.settings)[0]
        self.assertEqual(result.page,2)
        self.assertEqual(result.kind,'pdf')
        self.assertIn('INV-77891',result.snippet)

    def test_scanned_pdf_ocr_retains_page_rectangles(self):
        path=self.source/'scan.pdf'
        with pymupdf.open() as doc:
            doc.new_page(); doc.save(path)
        with patch('content.ocr',return_value=('Invoice 88321',[['88321',20,40,80,20]])):
            result=list(extract(path,dict(self.settings,ocr_enabled=True),read_text,lambda:None))[0]
        self.assertEqual(result.page,1)
        self.assertEqual(result.boxes,[['88321',10,20,40,10]])

    def test_exact_text_at_end_of_long_document_is_not_discarded(self):
        path=self.source/'long.txt'
        path.write_text('ordinary document prose '*2400+' END-992817',encoding='utf-8')
        self.engine.index(self.settings)
        self.assertIn('END-992817',self.engine.search('END-992817',self.settings)[0].snippet)

    def test_disabled_code_content_never_enters_general_search(self):
        (self.source/'module.py').write_text('def very_unique_retry():\n    return "HIDDEN_SENTINEL_12451"\n')
        self.engine.index(self.settings)
        self.assertEqual(self.engine.search('HIDDEN_SENTINEL_12451',self.settings),[])
        with self.assertRaisesRegex(ValueError,'Enable code'):
            self.engine.search('code: retry',self.settings)

    def test_opt_in_code_only_projects_and_function_location(self):
        (self.source/'module.py').write_text('import time\n\ndef backoff_call():\n    """Retry failed requests with backoff."""\n    time.sleep(2)\n')
        settings=dict(self.settings,roots=[],code_enabled=True,code_roots=[str(self.source)])
        self.engine.index(settings)
        result=self.engine.search('code: backoff',settings)[0]
        self.assertEqual((result.symbol,result.line,result.end_line),('backoff_call',3,5))
        self.assertEqual(self.engine.search('backoff',settings),[])

    def test_javascript_named_function_handles_braces_in_strings(self):
        path=self.source/'client.js'
        path.write_text('const unrelated = 1;\nasync function retryCall(request) {\n const text = "}";\n // } comment\n return request();\n}\nfunction other() { return 4; }\n')
        segments=list(code_segments(path))
        self.assertEqual(segments[0].symbol,'retryCall')
        self.assertEqual((segments[0].line,segments[0].end_line),(2,6))
        self.assertEqual(segments[1].symbol,'other')

    def test_notebook_indexes_only_code_cells(self):
        path=self.source/'analysis.ipynb'
        path.write_text(json.dumps({'cells':[{'cell_type':'markdown','source':['PRIVATE MARKDOWN']},{'cell_type':'code','source':['def retry(): pass'],'outputs':[{'text':['PRIVATE OUTPUT']}]}]}))
        segments=list(code_segments(path))
        self.assertEqual(segments[0].text,'def retry(): pass')
        self.assertNotIn('PRIVATE',segments[0].text)

    def test_gitignore_and_nested_project_boundaries(self):
        subprocess.run(['git','init',str(self.source)],capture_output=True,check=True,creationflags=subprocess.CREATE_NO_WINDOW)
        (self.source/'.gitignore').write_text('ignored.py\n')
        (self.source/'ignored.py').write_text('def ignored(): pass')
        (self.source/'included.py').write_text('def included(): pass')
        paths=list(project_files(self.source))
        self.assertIn(self.source/'included.py',paths)
        self.assertNotIn(self.source/'ignored.py',paths)

    def test_source_parent_names_do_not_exclude_explicit_selection(self):
        nested=self.source/'windows'; nested.mkdir()
        (nested/'notes.txt').write_text('explicitly selected application sources')
        self.engine.index(dict(self.settings,roots=[str(nested)]))
        self.assertTrue(self.engine.search('application',self.settings))

    def test_changed_content_options_reextract_unchanged_files(self):
        (self.source/'notes.txt').write_text('first text')
        self.engine.index(self.settings)
        with patch('engine.read_text',return_value='updated OCR configuration'):
            self.engine.index(dict(self.settings,ocr_languages='eng+ara'))
        self.assertTrue(self.engine.search('updated',self.settings))

    def test_file_failure_skips_partial_content_and_continues(self):
        path=self.source/'notes.txt'; path.write_text('old invoice INV-1299')
        self.engine.index(self.settings)
        path.write_text('bad replacement')
        (self.source/'good.txt').write_text('healthy content after failure')
        from content import Segment
        original=__import__('engine').extract
        def extract(path,*args):
            if path.name=='notes.txt':
                yield Segment(text='partial text that must not commit')
                raise RuntimeError('decode failed')
            yield from original(path,*args)
        with patch('engine.extract',side_effect=extract),self.assertLogs('vectordash',level='WARNING') as logs:
            summary=self.engine.index(self.settings)
        self.assertIn('1 skipped',summary)
        self.assertIn('decode failed',logs.output[0])
        self.assertTrue(self.engine.search('healthy',self.settings))
        self.assertEqual(self.engine.search('partial',self.settings),[])
        self.assertTrue(self.engine.search('notes',self.settings))

    def test_duplicate_source_selection_does_not_duplicate_segments(self):
        (self.source/'notes.txt').write_text('selected twice')
        self.engine.index(dict(self.settings,roots=[str(self.source),str(self.source)]))
        with self.engine.connect() as db:
            self.assertEqual(db.execute('SELECT count(*) FROM segments').fetchone()[0],1)

    def test_multilingual_literal_text_and_punctuation(self):
        (self.source/'notes.txt').write_text('مرحبا رقم الفاتورة INV-5532. বাংলা নোট।',encoding='utf-8')
        self.engine.index(self.settings)
        for query in ('مرحبا','INV-5532','বাংলা'):
            self.assertTrue(self.engine.search(query,self.settings),query)

    def test_ocr_screenshot_exact_identifiers(self):
        if not ocr_program(self.settings):
            self.skipTest('Tesseract unavailable')
        image=Image.new('RGB',(1000,350),'white')
        draw=ImageDraw.Draw(image)
        draw.text((30,50),'Payment declined error',font=ImageFont.truetype('C:/Windows/Fonts/arial.ttf',40),fill='black')
        draw.text((30,150),'Invoice INV-2026-00891',font=ImageFont.truetype('C:/Windows/Fonts/arial.ttf',40),fill='black')
        image.save(self.source/'123.png')
        self.engine.index(dict(self.settings,ocr_enabled=True))
        result=self.engine.search('INV-2026-00891',self.settings)[0]
        self.assertEqual(result.kind,'image')
        self.assertTrue(result.boxes)

    def test_projector_fingerprint_invalidates_vectors(self):
        model=self.home/'model.gguf'; model.write_bytes(b'model')
        projector=self.home/'mmproj.gguf'; projector.write_bytes(b'encoder')
        settings=dict(self.settings,semantic=True,model=str(model),projector=str(projector))
        before=model_signature(settings)
        projector.write_bytes(b'a different encoder')
        self.assertNotEqual(before,model_signature(settings))

    def test_cpu_encoder_and_pooling_configuration(self):
        args=command(dict(self.settings,server='server.exe',model='model.gguf',projector='mmproj.gguf'))
        self.assertIn('--no-mmproj-offload',args)
        self.assertEqual(args[args.index('--pooling')+1],'mean')
        self.assertEqual(args[args.index('--mmproj')+1],'mmproj.gguf')

    def test_media_request_contains_raw_payload_without_filename(self):
        props=io.BytesIO(json.dumps({'media_marker':'MEDIA'}).encode())
        vector=io.BytesIO(json.dumps({'data':[{'embedding':[.3,.4]}]}).encode())
        with patch('vectordash_core.urllib.request.urlopen',side_effect=[props,vector]) as requests:
            embedding_request(b'actual image bytes')
        request=requests.call_args_list[1].args[0]
        payload=json.loads(request.data)['input']
        self.assertEqual(payload['prompt_string'],'MEDIA')
        self.assertEqual(payload['multimodal_data'],['YWN0dWFsIGltYWdlIGJ5dGVz'])

    def test_prompted_text_and_media_are_indexed_as_separate_vectors(self):
        path=self.source/'photo.jpg'; path.write_bytes(b'fixture')
        segment=Segment(kind='image',text='Invoice 234',media=b'image')
        requests=[]
        def runtime(value):
            requests.append(value)
            return [1.,0.]
        with patch('engine.embedding_request',side_effect=runtime):
            vectors=self.engine.infer(segment,path,lambda:True)
        self.assertEqual(len(vectors),2)
        self.assertEqual(requests[0],b'image')
        self.assertEqual(requests[1],'title: photo.jpg | text: Invoice 234')

    def test_chunks_cover_complete_text(self):
        text=''.join(str(i%10) for i in range(20000))
        passages=list(chunks(text))
        self.assertEqual(passages[0],text[:1800])
        self.assertTrue(passages[-1].endswith(text[-100:]))
        self.assertEqual(len(passages),13)

if __name__=='__main__':
    unittest.main()
