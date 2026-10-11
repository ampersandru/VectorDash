"""Cross-media ranking and optional high-detail/PDF visual extraction regressions."""
import io
import json
import math
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import numpy as np
from PIL import Image
import pymupdf
from engine import Engine, DEFAULTS
from content import extract, image_bytes
from vectordash_core import command

class VisualSearchTests(unittest.TestCase):
    def test_cross_media_cutoff_preserves_photo_and_exact_words(self):
        with tempfile.TemporaryDirectory() as temp:
            engine=Engine(Path(temp))
            definitions=[('photo.jpg','image','',.70,'media'),('note.txt','text','',.91,'text'),('weak.txt','text','',.65,'text'),('exact.txt','text','zebra',.10,'text')]
            records=[(i+1,str(Path(temp)/name),kind,text,text,json.dumps({'embedding_source':channel})) for i,(name,kind,text,score,channel) in enumerate(definitions)]
            matrix=np.array([[score,math.sqrt(1-score**2)] for _,_,_,score,_ in definitions],dtype=np.float32)
            with patch.object(engine,'cached_index',return_value=(records,{2:(np.arange(4),matrix)},{'signature':'fixture'})),patch('engine.model_signature',return_value='fixture'),patch('engine.verify_server_model'),patch('engine.embed',return_value=[1,0]):
                found=engine.search('zebra',dict(DEFAULTS,semantic=True))
                names={Path(item.path).name for item in found}
                self.assertIn('photo.jpg',names)
                self.assertIn('note.txt',names)
                self.assertIn('exact.txt',names)
                self.assertNotIn('weak.txt',names)
                filtered=engine.search('zebra',dict(DEFAULTS,semantic=True,search_category='Photos / screenshots'))
                self.assertEqual([Path(item.path).name for item in filtered],['photo.jpg'])

    def test_visual_detail_changes_pixels_and_local_token_budget(self):
        image=Image.new('RGB',(3000,2000))
        for detail,size,tokens in [('Balanced',1024,'280'),('High detail',2048,'1120')]:
            settings=dict(DEFAULTS,semantic=True,visual_detail=detail,projector='encoder.gguf')
            with Image.open(io.BytesIO(image_bytes(image,settings))) as result:
                self.assertEqual(max(result.size),size)
            args=command(settings)
            self.assertEqual(args[args.index('--image-max-tokens')+1],tokens)
        settings=dict(DEFAULTS,projector='encoder.gguf',index_images=False,index_audio=False,index_video=False,pdf_visuals=True)
        self.assertIn('--mmproj',command(settings))

    def test_pdf_visual_option_keeps_text_and_page_location(self):
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'chart.pdf'
            with pymupdf.open() as doc:
                page=doc.new_page();page.insert_text((72,72),'Quarterly sales chart')
                page.draw_rect(pymupdf.Rect(72,100,250,300),color=(.6,.2,.8),fill=(.6,.2,.8))
                doc.save(path)
            settings=dict(DEFAULTS,semantic=True,ocr_enabled=False)
            baseline=list(extract(path,settings,lambda _: '',lambda:None))
            detailed=list(extract(path,dict(settings,pdf_visuals=True),lambda _: '',lambda:None))
            self.assertEqual(len(baseline),1);self.assertFalse(baseline[0].media)
            self.assertEqual(len(detailed),2)
            self.assertIn('Quarterly',detailed[0].text)
            self.assertTrue(detailed[1].media);self.assertEqual(detailed[1].page,1)
            no_model=list(extract(path,dict(settings,pdf_visuals=True,semantic=False),lambda _: '',lambda:None))
            self.assertEqual(len(no_model),1)

if __name__=='__main__': unittest.main()
