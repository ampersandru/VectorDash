import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from engine import Engine,DEFAULTS

class PerformanceTests(unittest.TestCase):
    def test_read_connection_reused_and_refreshed_after_write(self):
        with tempfile.TemporaryDirectory() as temp:
            engine=Engine(Path(temp))
            with engine.connect(readonly=True) as first:
                self.assertEqual(first.execute('select count(*) from files').fetchone()[0],0)
            with engine.connect(readonly=True) as second:
                if os.name=='nt':self.assertIs(first,second)
            with engine.connect() as writer:
                writer.execute('insert into files(path,text,vector,mtime,size) values(?,?,?,?,?)',('fixture.txt','test','[]',1,1))
            with engine.connect(readonly=True) as current:
                self.assertEqual(current.execute('select count(*) from files').fetchone()[0],1)
                if os.name=='nt':
                    self.assertIsNot(second,current)
                    with self.assertRaises(Exception):current.execute('delete from files')

    def test_unsupported_filename_keeps_exact_search_without_parser_or_embedding(self):
        with tempfile.TemporaryDirectory() as temp:
            home=Path(temp);source=home/'source';source.mkdir()
            path=source/'invoice-48291.exe';path.write_bytes(b'not an executable')
            engine=Engine(home);settings=dict(DEFAULTS,semantic=True,roots=[str(source)],ocr_enabled=False)
            with patch('engine.model_signature',return_value='fixture'),patch('engine.verify_server_model'),patch.object(engine,'infer',side_effect=AssertionError('unnecessary model call')),patch('engine.isolated_extract',side_effect=AssertionError('unnecessary parser')):
                engine.index(settings)
            self.assertEqual(engine.search('invoice-48291',dict(settings,semantic=False,search_names=True))[0].path,str(path))

if __name__=='__main__':unittest.main()
