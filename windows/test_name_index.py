"""Names-only indexing, source mode changes and model-free scoped retrieval."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from engine import Engine,DEFAULTS
from name_index import normalized,replace_catalog

class NameIndexTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.home=Path(self.temp.name);self.source=self.home/'source';self.source.mkdir()
        (self.home/'data').mkdir()
        self.engine=Engine(self.home/'data')
        self.settings=dict(DEFAULTS,roots=[str(self.source)],semantic=True,model='missing.gguf',ocr_enabled=False,source_modes={normalized(self.source):'names'})

    def test_quick_index_never_extracts_or_calls_model_and_finds_folders_partial_unicode(self):
        folder=self.source/'Holiday Photos';folder.mkdir()
        file=folder/'Bengali বাংলা Invoice_4001.pdf';file.write_text('private text should not be indexed')
        with patch('engine.verify_server_model',side_effect=AssertionError('model called')),patch('engine.extract',side_effect=AssertionError('extract called')),patch('engine.isolated_extract',side_effect=AssertionError('parser called')),patch('engine.Engine.infer',side_effect=AssertionError('embedding called')):
            self.engine.index(self.settings)
        with self.engine.connect(readonly=True) as db:
            self.assertTrue(all(not row[0] and row[1]=='[]' for row in db.execute('SELECT text,vector FROM segments')))
        with patch.object(self.engine,'cached_index',side_effect=AssertionError('vector cache loaded')),patch('engine.embedding_request',side_effect=AssertionError('query embedded')):
            for query in ('Invoice_400','বাংলা','Bengali'):
                self.assertEqual(self.engine.search(query,dict(self.settings,search_mode='Names'))[0].path,str(file))
            folders=self.engine.search('Holiday',dict(self.settings,search_mode='Names',search_category='Folders'))
            self.assertEqual(folders[0].path,str(folder));self.assertEqual(folders[0].kind,'folder')
        self.assertEqual(self.engine.indexed_folder_count(),2)
        version=self.engine.storage_version
        self.engine.index(self.settings)
        self.assertEqual(self.engine.storage_version,version)

    def test_mode_changes_reindex_only_selected_source_and_remove_content_in_quick_mode(self):
        nested=self.source/'full';nested.mkdir();other=self.source/'quick';other.mkdir()
        full=nested/'answer.txt';full.write_text('contentonly platypus')
        quick=other/'answer.txt';quick.write_text('do not parse this')
        settings=dict(self.settings,semantic=False,roots=[str(self.source),str(nested)],source_modes={normalized(self.source):'names',normalized(nested):'full'})
        self.engine.index(settings)
        self.assertEqual(self.engine.search('platypus',dict(settings,search_mode='Semantic'))[0].path,str(full))
        self.assertFalse(self.engine.search('parse',dict(settings,search_mode='Semantic')))
        settings['source_modes']={normalized(self.source):'names',normalized(nested):'names'}
        self.engine.index(settings)
        self.assertFalse(self.engine.search('platypus',dict(settings,search_mode='Semantic')))
        settings['source_modes']={normalized(self.source):'names',normalized(nested):'full'}
        states=[];self.engine.index(settings,file_progress=states.append)
        planned=next(state for state in states if state['phase']=='indexing')
        self.assertEqual(planned['needs_index'],1);self.assertEqual(planned['reused'],1)
        self.assertTrue(self.engine.search('platypus',dict(settings,search_mode='Semantic')))

    def test_source_filter_all_some_none_and_content_scope(self):
        other=self.home/'other';other.mkdir()
        first=self.source/'invoice.txt';first.write_text('invoice exactword')
        second=other/'invoice.txt';second.write_text('invoice exactword')
        settings=dict(self.settings,semantic=False,roots=[str(self.source),str(other)],source_modes={})
        self.engine.index(settings)
        for mode in ('Names','Semantic','Both'):
            query='invoice' if mode=='Names' else 'exactword'
            self.assertEqual(len(self.engine.search(query,dict(settings,search_mode=mode,search_sources=None))),2)
            self.assertEqual([r.path for r in self.engine.search(query,dict(settings,search_mode=mode,search_sources=[str(other)]))],[str(second)])
            self.assertFalse(self.engine.search(query,dict(settings,search_mode=mode,search_sources=[])))

    def test_exclusions_and_individual_file_sources(self):
        file=self.source/'only_file.txt';file.write_text('no content indexing')
        settings=dict(self.settings,roots=[str(file)],source_modes={normalized(file):'names'},search_mode='Names')
        self.engine.index(settings)
        self.assertEqual(self.engine.search('only_file',settings)[0].path,str(file))
        self.assertFalse(self.engine.search('only_file',dict(settings,search_excluded_types='.txt')))
        self.assertFalse(self.engine.search('only_file',dict(settings,search_excluded_files=[str(file)])))
        file.unlink();other=self.source/'retained.txt';other.write_text('retained')
        settings=dict(self.settings,search_mode='Names')
        self.engine.index(settings)
        self.assertFalse(self.engine.search('only_file',settings));self.assertTrue(self.engine.search('retained',settings))

    def test_canceled_update_preserves_name_catalog(self):
        first=self.source/'original.txt';first.write_text('original')
        self.engine.index(self.settings)
        version=self.engine.storage_version
        second=self.source/'new.txt';second.write_text('new')
        def cancel(state):
            if state['phase']=='indexing':self.engine.cancel.set()
        with self.assertRaises(InterruptedError):self.engine.index(self.settings,file_progress=cancel)
        self.engine.cancel.clear()
        self.assertEqual(self.engine.storage_version,version)
        settings=dict(self.settings,search_mode='Names')
        self.assertTrue(self.engine.search('original',settings))
        self.assertFalse(self.engine.search('new',settings))

    def test_hidden_and_secret_names_stay_excluded(self):
        import os,ctypes
        (self.source/'secrets.txt').write_text('credentials')
        hidden=self.source/'hidden.txt';hidden.write_text('hidden')
        if os.name=='nt':
            ctypes.windll.kernel32.SetFileAttributesW(str(hidden),2)
            self.addCleanup(lambda:ctypes.windll.kernel32.SetFileAttributesW(str(hidden),128))
        else:hidden.rename(self.source/'.hidden.txt')
        self.engine.index(self.settings)
        settings=dict(self.settings,search_mode='Names')
        self.assertFalse(self.engine.search('secrets',settings))
        self.assertFalse(self.engine.search('hidden',settings))

    def test_filter_before_limit_and_literal_wildcard_punctuation(self):
        selected=self.source/'chosen';selected.mkdir()
        rows=[(str(self.source/f'invoice_{i}.txt'),f'invoice_{i}.txt',str(self.source),'file',str(self.source/f'invoice_{i}.txt').casefold()) for i in range(1000)]
        special=selected/'invoice_%_4001.txt'
        rows.append((str(special),special.name,str(selected),'file',str(special).casefold()))
        with self.engine.connect() as db:replace_catalog(db,rows)
        settings=dict(self.settings,search_mode='Names',search_sources=[str(selected)])
        self.assertEqual(self.engine.search('invoice',settings)[0].path,str(special))
        self.assertEqual(self.engine.search('%_4001',settings)[0].path,str(special))

if __name__=='__main__':unittest.main()
