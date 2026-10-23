"""Regenerate the offline settings label index after changing settings panels."""
import json,pathlib,re
root=pathlib.Path(__file__).resolve().parents[1]
groups={
'app.rs':{'sources':(1,'Your sources'),'models':(2,'Hardware'),'components_ui':(2,'Status'),'speech_status_ui':(2,'Status'),'settings':(4,'Settings'),'embedding_settings':(5,'Embedding Model'),'dictation_settings':(5,'Dictation Model'),'local_tts_ui':(5,'Voice Model'),'llm':(5,'Models and AI'),'scheduling':(3,'Scheduling'),'about':(7,'About VectorDash'),'logs':(6,'Logs')},
'app/manager.rs':{'trail_color_settings':(4,'Working trail colors'),'ai_search_defaults_settings':(4,'AI chat defaults'),'shortcut_settings':(4,'Keyboard shortcuts'),'clipboard_settings':(9,'Clipboard')},
'app/web.rs':{'search_order_settings':(4,'Search and dictation'),'web_settings':(8,'Web')},
'app/control_tools.rs':{'control_settings':(5,'Computer and browser use — BETA')}}
strong={'Decision model — BETA','Working trail colors','AI chat defaults','Screen-area AI defaults','Capture','Retention and storage','Search and paste','Saved items and cleanup'}
skip={'Local speech files installed','Selected local speech model needs downloading','Local speech model loaded','Refreshing voices…','File monitoring is off','Running as administrator','Parakeet TDT 0.6B v2 · English speech-to-text','No supported browser found.','Local','Remote','CPU','GPU (DirectML)','Browse…','Choose runtime…','Refresh','Remove','Clear','Install','Uninstall','Test','Select a model','Select model','Select voice','items','Saved',' tokens','Immediately','fp16','8-bit'}
entries=[]
for file,functions in groups.items():
 source=(root/'src'/file).read_text(encoding='utf-8');methods=list(re.finditer(r'^    (?:pub\(super\) )?fn (\w+)\(',source,re.M))
 for i,m in enumerate(methods):
  if m.group(1) not in functions:continue
  page,section=functions[m.group(1)];body=source[m.end():methods[i+1].start()if i+1<len(methods)else len(source)]
  for token in re.finditer(r'\bt\(\s*("(?:\\.|[^"\\])*")',body):
   title=json.loads(token.group(1));before=body[max(0,token.start()-55):token.start()]
   if re.search(r'ui\.heading\(\s*$',before) or title in strong:section=title
   if re.search(r'\.(?:weak|on_hover_text)\(\s*$',before) or title.endswith('.') or title in skip or not 3<=len(title)<=100 or not any(c.isalpha()for c in title) or '{'in title:continue
   entries.append((page,section,title,''))
extras=[
(2,'Status','Speech and model resources','tts speech kokoro paradee memory ram vram disk storage uninstall downloads'),
(2,'Status','llama-server','local runtime install uninstall downloads'),
(2,'Status','EmbeddingGemma 2','embedding model install uninstall downloads'),
(2,'Status','Dictation','parakeet transcription local remote install uninstall downloads'),
(2,'Status','Photos / audio / video encoder','projector mmproj media install uninstall downloads'),
(2,'Status','Tesseract OCR','optical character recognition text install uninstall downloads'),
(2,'Status','FFmpeg','media video decoder install uninstall downloads'),
(2,'Status','PDF renderer','preview pdfium install uninstall'),
(2,'Status','Camera RAW decoder','photo image raw install uninstall'),
(4,'Settings','Saved searches','rename delete saved query'),
(4,'Search and dictation','Default search mode','filenames semantic hybrid names full'),
(4,'Search and dictation','Performance mode','low memory balanced performance vector cache'),
(4,'Appearance','Theme','system dark light appearance colors'),
(4,'Startup and window behavior','Global search hotkey','keyboard shortcut popup launch'),
(4,'Keyboard shortcuts','Screenshot shortcut','screen area capture ai snip clipboard'),
(5,'Voice Model','Local speech model','tts kokoro paradee fp16 q8 v2f v2g lite download voice'),
(5,'Voice Model','Unload speech model','tts memory idle minutes immediately'),
(5,'Voice Model','Speech device','tts cpu gpu directml'),
(5,'Dictation Model','Dictation provider','remote local parakeet server transcription api'),
(5,'Embedding Model','Embedding provider','local remote embeddinggemma server url api'),
(5,'Decision model — BETA','Decision provider','qev local remote llama-server api url'),
(5,'Voice Model','Speech provider','tts local remote api url'),
(9,'Retention and storage','Text history','clipboard days retention forever'),
(9,'Retention and storage','Image history','clipboard days retention forever'),
(9,'Retention and storage','Maximum history items (0 = unlimited)','clipboard history limits'),
(9,'Retention and storage','History storage MiB (0 = unlimited)','clipboard disk memory storage limits'),
(9,'Retention and storage','Saved storage MiB (0 = unlimited)','clipboard saved disk storage limits'),
(9,'Capture','Excluded applications','clipboard privacy passwords executable'),
(8,'Web','SearXNG instance URL','web search engine self hosted'),
(8,'Web','Degoog instance URL','web search engine self hosted'),
(8,'Web','Brave Search API key','web search engine authentication'),
(3,'Scheduling','Indexing schedule','daily weekly interval idle time'),
]
entries+=extras
seen=set();out=['// Generated by native/scripts/generate_settings_catalog.py.','pub(super) const ENTRIES: &[Entry] = &[']
for page,section,title,aliases in entries:
 key=(page,section,title)
 if key in seen:continue
 seen.add(key)
 aliases+=' '+{'Status':'resource installed ram vram memory disk storage','Voice Model':'tts speech read aloud kokoro paradee','Dictation Model':'dictation parakeet microphone transcribe','Decision model — BETA':'qev decision','Keyboard shortcuts':'hotkey shortcut keyboard','Appearance':'theme transparency color','Retention and storage':'clipboard retention storage'}.get(section,'')
 out.append('    Entry { page: '+str(page)+', section: '+json.dumps(section,ensure_ascii=False)+', title: '+json.dumps(title,ensure_ascii=False)+', aliases: '+json.dumps(aliases.strip(),ensure_ascii=False)+' },')
out.append('];')
(root/'src/app/settings_catalog.rs').write_text('\n'.join(out)+'\n',encoding='utf-8')
print('Indexed',len(seen),'settings labels')
