"""Metadata-only file/folder catalog and model-free substring search."""
import os
import re
from pathlib import Path


def normalized(path):
    return os.path.normcase(os.path.abspath(path))


def under(path,root):
    path,root=normalized(path),normalized(root)
    return path==root or path.startswith(root.rstrip(os.sep)+os.sep)


def source_mode(path,settings):
    modes=settings.get('source_modes',{})
    matches=[root for root in settings.get('roots',[]) if under(path,root)]
    root=max(matches,key=len,default='')
    return modes.get(normalized(root),modes.get(root,'full'))


def in_scope(path,settings):
    selected=settings.get('search_sources')
    return selected is None or any(under(path,root) for root in selected)


def schema(db):
    db.execute('CREATE TABLE IF NOT EXISTS names(path TEXT PRIMARY KEY,name TEXT,parent TEXT,kind TEXT,folded TEXT,base TEXT)')
    if 'base' not in {row[1] for row in db.execute('PRAGMA table_info(names)')}:
        db.execute("ALTER TABLE names ADD COLUMN base TEXT DEFAULT ''")
        db.execute('UPDATE names SET base=lower(name)')
    columns={row[1] for row in db.execute('PRAGMA table_info(names)')}
    for name,definition in [('mtime','INTEGER DEFAULT -1'),('size','INTEGER DEFAULT -1'),('index_mode',"TEXT DEFAULT 'full'")]:
        if name not in columns:db.execute('ALTER TABLE names ADD COLUMN '+name+' '+definition)
    existing=db.execute("SELECT sql FROM sqlite_master WHERE name='name_words'").fetchone()
    if existing and "content='names'" not in existing[0]:
        db.execute('DROP TABLE name_words')
    db.execute("CREATE VIRTUAL TABLE IF NOT EXISTS name_words USING fts5(folded,content='names',content_rowid='rowid',tokenize='trigram')")
    if existing and "content='names'" not in existing[0]:
        db.execute("INSERT INTO name_words(name_words) VALUES('rebuild')")


def replace_catalog(db,rows):
    db.execute("INSERT INTO name_words(name_words) VALUES('delete-all')")
    db.execute('DELETE FROM names')
    db.executemany('INSERT INTO names VALUES(?,?,?,?,?,?,?,?,?)',(tuple(row[:5])+(row[1].casefold(),)+(tuple(row[5:]) if len(row)>5 else (-1,-1,'full')) for row in rows))
    db.execute('INSERT INTO name_words(rowid,folded) SELECT rowid,folded FROM names')


def rows_for_search(db,query,settings,category_suffixes):
    # Literal substring matching, including partial filenames and punctuation.
    # Trigram FTS narrows longer terms; tiny queries fall back to the name table.
    tokens=re.findall(r'\S+',query.casefold())
    terms=[token for token in tokens if len(token)>=3]
    expression=' AND '.join('"'+term.replace('"','""')+'"' for term in terms)
    filters=[];params=[]
    if terms:
        filters.append('n.rowid IN (SELECT rowid FROM name_words WHERE name_words MATCH ?)')
        params.append(expression)
    for token in tokens:
        filters.append("n.folded LIKE ? ESCAPE '\\'")
        params.append('%'+token.replace('\\','\\\\').replace('%','\\%').replace('_','\\_')+'%')
    if not filters:return []
    def like(value):
        return value.replace('\\','\\\\').replace('%','\\%').replace('_','\\_')
    selected=settings.get('search_sources')
    if selected is not None:
        if not selected:return []
        scope=[]
        for root in selected:
            folded=normalized(root).casefold()
            scope.append("(n.folded=? OR n.folded LIKE ? ESCAPE '\\')")
            params.extend([folded,like(folded.rstrip(os.sep)+os.sep)+'%'])
        filters.append('('+' OR '.join(scope)+')')
    for root in settings.get('excluded_folders',[]):
        folded=normalized(root).casefold()
        filters.append("NOT (n.folded=? OR n.folded LIKE ? ESCAPE '\\')")
        params.extend([folded,like(folded.rstrip(os.sep)+os.sep)+'%'])
    hidden=settings.get('search_excluded_files',[])
    if hidden:
        filters.append('n.folded NOT IN ('+','.join('?' for _ in hidden)+')')
        params.extend(normalized(path).casefold() for path in hidden)
    category=settings.get('search_category','All files')
    if category=='Folders':filters.append("n.kind='folder'")
    elif category!='All files':
        filters.append("n.kind='file'")
        if category in category_suffixes:
            endings=[]
            for suffix in category_suffixes[category]:
                endings.append('substr(n.folded,?)=?');params.extend([-len(suffix),suffix])
            filters.append('('+' OR '.join(endings)+')')
    exclusions=settings.get('search_excluded_types','')+' '+settings.get('index_excluded_types','')
    for value in re.split(r'[,;\s]+',exclusions):
        if not value:continue
        suffix=value.casefold() if value.startswith('.') else '.'+value.casefold()
        filters.append("(n.kind='folder' OR substr(n.folded,?)!=?)");params.extend([-len(suffix),suffix])
    ordering='CASE WHEN n.base=? OR n.base LIKE ? ESCAPE '+"'\\'"+' THEN 2 WHEN n.base LIKE ? ESCAPE '+"'\\'"+' THEN 1 ELSE 0 END DESC'
    params.extend([query.casefold(),like(query.casefold())+'.%', '%'+like(query.casefold())+'%'])
    sql='SELECT n.path,n.name,n.parent,n.kind FROM names n WHERE '+' AND '.join(filters)+' ORDER BY '+ordering+',length(n.path),n.path LIMIT 400'
    return db.execute(sql,params)
