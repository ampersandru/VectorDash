"""Performance regressions: bounded caches, mixed catalogs, and source activity."""
import json
import os
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import patch
import numpy as np
from engine import Engine,DEFAULTS
from name_index import normalized
from scan_plan import ScanPlan,ScanPolicy
from search_resources import profile
from disk_search import candidates

class ResourceProfileTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.home=Path(self.temp.name);self.root=self.home/'source';self.root.mkdir();self.data=self.home/'data';self.data.mkdir()
        self.engine=Engine(self.data)
        self.settings=dict(DEFAULTS,semantic=False,ocr_enabled=False,roots=[str(self.root)],source_modes={normalized(self.root):'names'})

    def test_cached_vectors_match_streamed_candidates_and_obey_budget(self):
        with self.engine.connect() as db:
            for i in range(900):
                kind='image' if i%2 else 'text';channel='media' if kind=='image' else 'text'
                db.execute('INSERT INTO segments VALUES(?,?,?,?,?,?)',(str(self.root/f'file{i}.jpg'),'files',kind,'test subject',json.dumps([1,i/900]),json.dumps({'embedding_source':channel})))
            db.execute('INSERT INTO words(rowid,path,text) SELECT rowid,path,text FROM segments')
        outputs={}
        for mode in ('Low memory','Balanced','Fast search'):
            self.engine.configure_performance(mode)
            settings=dict(self.settings,performance_profile=mode)
            rows,_,meta=candidates(self.engine,'files','a subject',[1,0],settings,{})
            outputs[mode]={row[0]:score for row,score in zip(rows,meta['_scores'])}
            self.assertLessEqual(self.engine.vector_cache.bytes,profile(mode)['vectors']*1024*1024)
        self.assertEqual(outputs['Low memory'].keys(),outputs['Balanced'].keys())
        for key in outputs['Low memory']:self.assertAlmostEqual(outputs['Low memory'][key],outputs['Balanced'][key],places=6)
        self.engine.configure_performance('Low memory');self.assertEqual(self.engine.vector_cache.bytes,0)

    def test_cache_falls_back_when_matrix_exceeds_selected_limit(self):
        with self.engine.connect() as db:
            db.execute("INSERT INTO segments VALUES('note.txt','files','text','words','[1,0]','{}')")
        from search_resources import PROFILES
        self.engine.configure_performance('Balanced')
        with patch.dict(PROFILES,{'Balanced':{'pages':8,'vectors':0.000001}}):
            with self.engine.connect(readonly=True) as db:self.assertIsNone(self.engine.vector_cache.get(self.engine,db,'files',2,'Balanced'))
        self.assertEqual(self.engine.vector_cache.bytes,0)

    def test_results_cache_is_bounded_isolated_and_invalidated(self):
        note=self.root/'forza.txt';note.write_text('note');self.engine.index(self.settings)
        settings=dict(self.settings,search_mode='Names')
        first=self.engine.search('forza',settings);first[0].reasons.append('caller edit')
        with patch.object(self.engine,'search_files',side_effect=AssertionError('cached search queried disk')):
            second=self.engine.search('forza',settings)
        self.assertNotIn('caller edit',second[0].reasons)
        for i in range(45):self.engine.search(f'query-{i}',settings)
        self.assertLessEqual(len(self.engine.result_cache),32);self.assertLessEqual(self.engine.result_cache_bytes,4*1024*1024)
        self.engine.clear_query_cache();self.assertFalse(self.engine.result_cache)
        self.engine.search('forza',settings);note.unlink();self.engine.index(self.settings)
        self.assertFalse(self.engine.search('forza',settings))

    def test_source_counts_track_additions_not_edits_and_expire(self):
        nested=self.root/'docs';nested.mkdir();(nested/'original.txt').write_text('content')
        settings=dict(self.settings,roots=[str(self.root),str(nested)])
        with patch('engine.time.time',return_value=10000):
            self.engine.index(settings)
            for root in settings['roots']:
                stats=self.engine.source_stats(settings)[normalized(root)];self.assertEqual(stats,{'files':1,'added_hour':1})
        with patch('engine.time.time',return_value=14000):
            self.assertEqual(self.engine.source_stats(settings)[normalized(self.root)]['added_hour'],0)
            (nested/'original.txt').write_text('changed existing file');(nested/'new.txt').write_text('new')
            self.engine.index(settings)
            for root in settings['roots']:self.assertEqual(self.engine.source_stats(settings)[normalized(root)],{'files':2,'added_hour':1})

    def test_removed_full_file_checks_use_path_index_among_quick_files(self):
        plan=ScanPlan(self.data)
        try:
            class Stat:st_mtime_ns=1;st_size=1
            for i in range(40000):plan.queue(self.root/f'quick{i}.bin','files',Stat(),'names')
            full=str(self.root/'full.txt');plan.queue(full,'files',Stat(),'full');plan.load_segments([(full,'files')])
            self.assertFalse(plan.full_keys_changed())
            plan.load_segments([(str(self.root/'removed.txt'),'files')]);self.assertTrue(plan.full_keys_changed())
            query="SELECT 1 FROM old_segments o WHERE NOT EXISTS(SELECT 1 FROM candidates c JOIN catalog n USING(key) WHERE n.path=o.path AND c.namespace=o.namespace AND (c.mode!='names' OR c.namespace='code')) LIMIT 1"
            explanation=' '.join(row[3] for row in plan.db.execute('EXPLAIN QUERY PLAN '+query))
            self.assertIn('catalog_path',explanation)
        finally:plan.close()

    def test_update_summary_does_not_count_retained_full_files_as_new(self):
        docs=self.root/'documents';docs.mkdir()
        (docs/'keep.txt').write_text('unchanged document')
        quick=self.root/'quick.bin';quick.write_bytes(b'one')
        settings=dict(self.settings,roots=[str(self.root),str(docs)],source_modes={normalized(self.root):'names',normalized(docs):'full'})
        self.engine.index(settings)
        quick.write_bytes(b'changed size')
        summary=self.engine.index(settings)
        self.assertTrue(summary.startswith('1 files indexed'),summary)
        self.assertEqual(self.engine.source_stats(settings)[normalized(self.root)]['files'],2)

    def test_compiled_exclusions_preserve_scope_and_churn_rules(self):
        hidden=self.root/'cache';selected=self.root/'documents'
        settings=dict(self.settings,excluded_folders=[str(hidden)],search_sources=[str(selected)])
        policy=ScanPolicy(settings)
        self.assertFalse(policy.search_allowed(hidden/'note.txt',settings))
        self.assertTrue(policy.search_allowed(selected/'note.txt',settings))
        self.assertFalse(policy.search_allowed(self.root/'other.txt',settings))

if __name__=='__main__':unittest.main()
