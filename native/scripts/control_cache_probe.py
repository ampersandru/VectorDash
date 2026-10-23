"""Replay synthetic native context-policy probes. No computer/browser actions are executed."""
import argparse,base64,ctypes,json,pathlib,time,urllib.request,urllib.parse
p=argparse.ArgumentParser();p.add_argument('--requests',required=True);p.add_argument('--settings',required=True);p.add_argument('--allow-host',required=True);p.add_argument('--output',required=True);a=p.parse_args()
s=json.loads(pathlib.Path(a.settings).read_text(encoding='utf-8-sig'));base=s['llm_url'].rstrip('/')
assert urllib.parse.urlsplit(base).hostname==a.allow_host,'Unexpected backend host'
headers={'Content-Type':'application/json'}
key=s.get('llm_key','')
if key.startswith('dpapi:'):
 class Blob(ctypes.Structure):_fields_=[('size',ctypes.c_ulong),('data',ctypes.POINTER(ctypes.c_ubyte))]
 encrypted=base64.b64decode(key[6:]);buffer=(ctypes.c_ubyte*len(encrypted)).from_buffer_copy(encrypted);source=Blob(len(encrypted),buffer);target=Blob()
 crypt=ctypes.WinDLL('crypt32',use_last_error=True);kernel=ctypes.WinDLL('kernel32')
 crypt.CryptUnprotectData.argtypes=[ctypes.POINTER(Blob),ctypes.c_void_p,ctypes.c_void_p,ctypes.c_void_p,ctypes.c_void_p,ctypes.c_ulong,ctypes.POINTER(Blob)]
 kernel.LocalFree.argtypes=[ctypes.c_void_p]
 assert crypt.CryptUnprotectData(ctypes.byref(source),None,None,None,None,1,ctypes.byref(target)),'Could not read account-protected API credential'
 try:key=ctypes.string_at(target.data,target.size).decode('utf-8')
 finally:kernel.LocalFree(target.data)
if key:headers['Authorization']='Bearer '+key
requests=json.loads(pathlib.Path(a.requests).read_text(encoding='utf-8'));results={}
try:
 for policy in ['rolling','checkpoint']:
  rows=[];previous=[]
  for index,body in enumerate(requests[policy],1):
   messages=body['messages'];same=0
   for left,right in zip(previous,messages):
    if left!=right:break
    same+=1
   rewritten=bool(previous) and same<len(previous);previous=messages
   body['model']=s['llm_model'];started=time.perf_counter()
   with urllib.request.urlopen(urllib.request.Request(base+'/chat/completions',data=json.dumps(body).encode(),headers=headers),timeout=120) as response:data=json.load(response)
   usage=data.get('usage') or {};details=usage.get('prompt_tokens_details') or {}
   row={'step':index,'input_tokens':usage.get('prompt_tokens'),'cached_tokens':details.get('cached_tokens'),'model_ms':round((time.perf_counter()-started)*1000,2),'context_rewritten':rewritten}
   rows.append(row);print(policy,json.dumps(row),flush=True)
  results[policy]=rows
  print(policy,'summary',json.dumps({'model_seconds':round(sum(r['model_ms'] for r in rows)/1000,2),'rewrites':sum(r['context_rewritten'] for r in rows),'input_tokens':sum(r['input_tokens'] or 0 for r in rows),'cached_tokens':sum(r['cached_tokens'] or 0 for r in rows)}),flush=True)
finally:pathlib.Path(a.output).write_text(json.dumps(results,indent=2),encoding='utf-8')
