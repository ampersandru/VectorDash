"""Bounded disk search: retain candidates, never the complete vector/text index."""
import heapq
import json
import os
from pathlib import Path
import re
import numpy as np
from name_index import in_scope
from security import excluded_path
from vectordash_core import safe

POOL=400
BATCH=256

def candidates(engine,namespace,query,vector,settings,metadata):
    tokens=re.findall(r'[\w]+(?:[-./:][\w]+)*',query.casefold())
    stop={'a','an','the','in','on','at','of','to','and','or','where','they','about','find','shows','show','me','with','is','it','image','images','photo','photos','picture','pictures','video','videos','clip','clips','audio','podcast','podcasts','document','documents','file','files'}
    tokens=[token for token in tokens if token not in stop] or tokens
    if not tokens:return [],{},metadata
    category=settings.get('search_category','All files')
    kinds={'Photos / screenshots':{'image'},'Documents':{'text','pdf','document','spreadsheet','slides'},'Audio / video':{'audio','video','video_audio'},'Code':{'code'},'Archives / models':{'archive','model_metadata','filename'}}.get(category,set())
    if category=='All files':
        if re.match(r'^(?:images?|photos?|pictures?)\s+(?:of\s+)?',query,re.I):kinds={'image'}
        elif any(phrase in query.casefold() for phrase in ('where they talk','podcast','in the recording')):kinds={'audio','video_audio'}
        elif re.search(r'\bin (?:a |the )?video\b',query,re.I):kinds={'video','video_audio'}
    excluded={value.lower() if value.startswith('.') else '.'+value.lower() for value in re.split(r'[,;\s]+',settings.get('search_excluded_types','')) if value}
    excluded_files={os.path.normcase(os.path.abspath(path)) for path in settings.get('search_excluded_files',[])}
    text_intent=bool(re.search(r'\d|[_:/-]|\b(?:error|text|screenshot|invoice|code|number)\b',query,re.I))
    def allowed(path,kind,channel,has_media):
        return kind!='filename' and (not kinds or kind in kinds) and policy.search_allowed(path,settings) and Path(path).suffix.lower() not in excluded and os.path.normcase(os.path.abspath(path)) not in excluded_files and safe(Path(Path(path).name)) and not (kind=='image' and channel=='text' and has_media and not text_intent)
    from scan_plan import ScanPolicy
    policy=ScanPolicy(settings)
    selected={};heaps={};totals={};counts={};path_means={};current=None
    q=np.array(vector,dtype=np.float32,copy=True)
    if q.size:q/=max(float(np.linalg.norm(q)),1e-30)
    with engine.connect(readonly=True,search=True) as db:
        db.execute('BEGIN')
        expression=' OR '.join('"'+token.replace('"','""')+'"' for token in tokens)
        if not settings.get('search_names',False):expression='text : ('+expression+')'
        # FTS5 implements a bounded top-k rank query; broad keywords no longer
        # populate an unbounded Python dictionary or set.
        for rowid,path,kind,text,location in db.execute("SELECT s.rowid,s.path,s.kind,s.text,s.location FROM words JOIN segments s ON s.rowid=words.rowid WHERE words MATCH ? AND s.namespace=? ORDER BY rank LIMIT ?",(expression,namespace,POOL)):
            meta=json.loads(location);channel=kind if meta.get('embedding_source')=='media' else 'text'
            has_media=bool(db.execute("SELECT 1 FROM segments WHERE path=? AND kind='image' AND json_extract(location,'$.embedding_source')='media' LIMIT 1",(path,)).fetchone()) if kind=='image' and channel=='text' and not text_intent else False
            if allowed(path,kind,channel,has_media):selected[rowid]=(rowid,path,kind,text,'',location)
        if q.size:
            dimensions={row[0] for row in db.execute('SELECT DISTINCT dimension FROM segment_vectors WHERE namespace=?',(namespace,))}
            if dimensions and len(q) not in dimensions:raise ValueError('Embedding dimensions do not match the index; rebuild with the selected model.')
            # Reading only float32 blobs avoids JSON lists and duplicate matrices.
            # Path order lets us calculate per-file channel means without a set
            # or dictionary containing all indexed files.
            cached=engine.vector_cache.get(engine,db,namespace,len(q),settings.get('performance_profile','Balanced'))
            if cached is not None:
                meta=cached['metadata'];group_ids=cached['groups']
                image_media={path for path,kind,channel in meta if kind=='image' and channel=='image'}
                permitted=np.asarray([allowed(path,kind,channel,path in image_media) for path,kind,channel in meta],dtype=np.bool_)
                scores=np.einsum('ij,j->i',cached['matrix'],q,optimize=False)
                maxima=np.full(len(meta),-np.inf,dtype=np.float32)
                np.maximum.at(maxima,group_ids,scores)
                # Collapse kind differences belonging to the same path/channel.
                by_file={}
                for i,(path,kind,channel) in enumerate(meta):
                    if permitted[i]:by_file[(path,channel)]=max(by_file.get((path,channel),-1),float(maxima[i]))
                for (_,channel),value in by_file.items():totals[channel]=totals.get(channel,0)+value;counts[channel]=counts.get(channel,0)+1
                for channel in {channel for _,_,channel in meta}:
                    groups=np.asarray([permitted[i] and item[2]==channel for i,item in enumerate(meta)],dtype=np.bool_)
                    indices=np.flatnonzero(groups[group_ids]);limit=min(POOL,len(indices))
                    if not limit:continue
                    chosen=np.argpartition(scores[indices],-limit)[-limit:]
                    heaps[channel]=[(float(scores[indices[j]]),int(cached['ids'][indices[j]]),None) for j in chosen]
            else:
                sql="SELECT id,path,kind,channel,vector FROM segment_vectors WHERE namespace=? AND dimension=?"
                params=[namespace,len(q)]
                if kinds:
                    sql+=' AND kind IN ('+','.join('?' for _ in kinds)+')';params.extend(sorted(kinds))
                if not text_intent:
                    sql+=" AND NOT(kind='image' AND channel='text' AND EXISTS(SELECT 1 FROM segment_vectors m WHERE m.path=segment_vectors.path AND m.channel='image'))"
                sql+=' ORDER BY path'
                cursor=db.execute(sql,params);current_allowed=False
                while rows:=cursor.fetchmany(BATCH):
                    matrix=np.asarray([np.frombuffer(row[4],dtype=np.float32) for row in rows],dtype=np.float32)
                    # Small batches do not benefit from an oversubscribed BLAS pool.
                    scores=np.einsum('ij,j->i',matrix,q,optimize=False)
                    for row,score in zip(rows,scores):
                        rowid,path,kind,channel,_=row;score=float(score)
                        if current!=path:
                            for group,value in path_means.items():totals[group]=totals.get(group,0)+value;counts[group]=counts.get(group,0)+1
                            current=path;path_means={}
                            current_allowed=policy.search_allowed(path,settings) and Path(path).suffix.lower() not in excluded and os.path.normcase(os.path.abspath(path)) not in excluded_files and safe(Path(Path(path).name))
                        if not current_allowed:continue
                        path_means[channel]=max(path_means.get(channel,-1),score)
                        # Only IDs enter the heap. Text and locations are fetched for
                        # the final candidate pool, avoiding a scan of document pages.
                        heap=heaps.setdefault(channel,[]);entry=(score,rowid,None)
                        if len(heap)<POOL:heapq.heappush(heap,entry)
                        elif entry[:2]>heap[0][:2]:heapq.heapreplace(heap,entry)
                for group,value in path_means.items():totals[group]=totals.get(group,0)+value;counts[group]=counts.get(group,0)+1
        scores_by_id={}
        for heap in heaps.values():
            for score,rowid,_ in heap:
                if rowid not in selected:selected[rowid]=None
                scores_by_id[rowid]=score
        # Keyword candidates below the semantic top-k still need their true
        # cosine score so exact identifiers retain the same ranking behavior.
        if q.size:
            for rowid in selected:
                if rowid in scores_by_id:continue
                stored=db.execute('SELECT vector FROM segment_vectors WHERE id=? AND dimension=?',(rowid,len(q))).fetchone()
                if stored:scores_by_id[rowid]=float(np.frombuffer(stored[0],dtype=np.float32)@q)
        missing=[rowid for rowid,record in selected.items() if record is None]
        if missing:
            for rowid,path,kind,text,location in db.execute('SELECT rowid,path,kind,text,location FROM segments WHERE rowid IN ('+','.join('?' for _ in missing)+')',missing):selected[rowid]=(rowid,path,kind,text,'',location)
        records=[record for record in selected.values() if record is not None]
        result=dict(metadata,_scores=[scores_by_id.get(record[0],0) for record in records],_modality_means={channel:total/counts[channel] for channel,total in totals.items()})
        result['_locations']=[json.loads(record[5]) for record in records]
        return records,{},result
