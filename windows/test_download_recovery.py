"""Rate-limit recovery, provider errors and download credential boundaries."""
import io,json,time,tempfile,threading,unittest,urllib.error,urllib.request
from pathlib import Path
from unittest.mock import patch,Mock
from setup_runtime import open_download,fetch_json,request_headers,SafeRedirect,DownloadError,download,_catalog_cache
from engine import Settings
class Response(io.BytesIO):
 status=200
 headers={}
def failure(code=403,headers=None,body=b'{"message":"API rate limit exceeded"}'):
 return urllib.error.HTTPError('https://api.github.com/example',code,'Denied',headers or {},io.BytesIO(body))
class DownloadRecoveryTests(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.home=Path(self.temp.name)
  self.cancel=threading.Event();_catalog_cache.clear()
 def test_short_rate_limit_retries_then_succeeds(self):
  with patch('setup_runtime.urllib.request.urlopen',side_effect=[failure(429,{'Retry-After':'0'}),Response(b'{}')]) as get:
   with open_download(urllib.request.Request('https://huggingface.co/api/models/test'),self.cancel) as r:self.assertEqual(r.read(),b'{}')
  self.assertEqual(get.call_count,2)
 def test_long_github_reset_is_actionable_and_does_not_wait(self):
  h={'X-RateLimit-Remaining':'0','X-RateLimit-Reset':str(time.time()+3600)}
  with patch('setup_runtime.urllib.request.urlopen',side_effect=failure(headers=h)) as get:
   with self.assertRaisesRegex(DownloadError,'GitHub rate limit.*GitHub token'):open_download(urllib.request.Request('https://api.github.com/example'),self.cancel)
  self.assertEqual(get.call_count,1)
 def test_permission_denied_is_not_retried_or_called_rate_limit(self):
  with patch('setup_runtime.urllib.request.urlopen',side_effect=failure(body=b'Forbidden')) as get:
   with self.assertRaisesRegex(DownloadError,'denied access'):open_download(urllib.request.Request('https://huggingface.co/example'),self.cancel)
  self.assertEqual(get.call_count,1)
 def test_retries_are_bounded(self):
  with patch('setup_runtime.urllib.request.urlopen',side_effect=lambda *a,**k:(_ for _ in ()).throw(failure(429,{'Retry-After':'0'}))) as get:
   with self.assertRaisesRegex(DownloadError,'Hugging Face rate limit'):open_download(urllib.request.Request('https://huggingface.co/example'),self.cancel)
  self.assertEqual(get.call_count,3)
 def test_cancel_interrupts_retry_wait(self):
  def progress(message):self.cancel.set()
  with patch('setup_runtime.urllib.request.urlopen',side_effect=failure(429,{'Retry-After':'1'})):
   with self.assertRaises(InterruptedError):open_download(urllib.request.Request('https://huggingface.co/example'),self.cancel,progress)
 def test_catalog_cached_across_calls_and_restarts(self):
  url='https://api.github.com/example'
  with patch('setup_runtime.urllib.request.urlopen',return_value=Response(b'{"assets":[]}')) as get:
   self.assertEqual(fetch_json(url,self.home),{'assets':[]})
   _catalog_cache.clear()
   self.assertEqual(fetch_json(url,self.home),{'assets':[]})
   self.assertEqual(get.call_count,1)
 def test_recent_catalog_survives_rate_limit_but_expired_does_not(self):
  url='https://api.github.com/example'
  with patch('setup_runtime.time.time',return_value=1000),patch('setup_runtime.urllib.request.urlopen',return_value=Response(b'{"assets":[]}')):fetch_json(url,self.home)
  with patch('setup_runtime.time.time',return_value=2000),patch('setup_runtime.urllib.request.urlopen',side_effect=lambda *a,**k:(_ for _ in ()).throw(failure(429,{'Retry-After':'999'}))):
   self.assertEqual(fetch_json(url,self.home),{'assets':[]})
  with patch('setup_runtime.time.time',return_value=100000),patch('setup_runtime.urllib.request.urlopen',side_effect=lambda *a,**k:(_ for _ in ()).throw(failure(429,{'Retry-After':'999'}))):
   with self.assertRaises(DownloadError):fetch_json(url,self.home)
 def test_tokens_are_provider_scoped(self):
  s={'github_token':'github-private','hf_token':'hf-private'}
  self.assertEqual(request_headers('https://api.github.com/x',s)['Authorization'],'Bearer github-private')
  self.assertEqual(request_headers('https://huggingface.co/x',s)['Authorization'],'Bearer hf-private')
  for url in ('http://huggingface.co/x','https://evil.test/x','https://huggingface.co.evil.test/x','https://huggingface.co:8443/x'):
   self.assertNotIn('Authorization',request_headers(url,s))
 def test_invalid_token_never_appears_in_error(self):
  with self.assertRaises(DownloadError) as raised:request_headers('https://huggingface.co/x',{'hf_token':'private\r\nvalue'})
  self.assertNotIn('private',str(raised.exception))
 def test_cross_origin_redirect_strips_auth_same_origin_preserves_it(self):
  req=urllib.request.Request('https://huggingface.co/model',headers={'Authorization':'Bearer private'})
  redirect=SafeRedirect()
  other=redirect.redirect_request(req,None,302,'Found',{},'https://cdn.example/model')
  self.assertFalse(other.has_header('Authorization'))
  same=redirect.redirect_request(req,None,302,'Found',{},'https://huggingface.co/other')
  self.assertTrue(same.has_header('Authorization'))
  with self.assertRaises(DownloadError):redirect.redirect_request(req,None,302,'Found',{},'http://huggingface.co/other')
 def test_download_tokens_are_account_protected(self):
  s=Settings(self.home);s.values.update(hf_token='hf-private',github_token='github-private');s.save()
  saved=s.path.read_text();self.assertNotIn('hf-private',saved);self.assertNotIn('github-private',saved)
  reopened=Settings(self.home);self.assertEqual(reopened.values['hf_token'],'hf-private');self.assertEqual(reopened.values['github_token'],'github-private')
 def test_resume_after_rate_limit_keeps_partial_and_checks_hash(self):
  import hashlib
  content=b'abcdef';target=self.home/'model.gguf';target.with_name('model.gguf.part').write_bytes(b'abc')
  part=Response(b'def');part.status=206;part.headers={'Content-Range':'bytes 3-5/6'}
  with patch('setup_runtime.urllib.request.urlopen',side_effect=[failure(429,{'Retry-After':'0'}),part]) as get:
   download('https://huggingface.co/model',target,6,hashlib.sha256(content).hexdigest(),self.cancel,lambda *a:None)
   self.assertEqual(get.call_args.args[0].get_header('Range'),'bytes=3-')
  self.assertEqual(target.read_bytes(),content)
 def test_setup_ui_preserves_runtime_on_model_limit_and_retries_encoder(self):
  from types import SimpleNamespace
  from desktop import Desktop
  from engine import DEFAULTS
  home=self.home
  widgets={key:Mock() for key in ('ocr_enabled','index_images','index_audio','index_video','mode')};widgets['mode'].currentText.return_value='Dedicated GPU'
  owner=SimpleNamespace(installing=False,indexing=False,setup_choices={key:Mock(isChecked=Mock(return_value=key in ('server','model','projector'))) for key in ('server','model','projector','ocr','ffmpeg')},widgets=widgets,settings=dict(DEFAULTS,hf_token='test-hf',github_token='test-github',device='Vulkan0',mode='Dedicated GPU'),save=Mock(return_value=True),component_installed=lambda key:False,model_process=None,quant_choice=Mock(currentData=Mock(return_value='Q8_0'),currentIndex=Mock(return_value=0)),runtime_choice=Mock(currentIndex=Mock(return_value=0)),runtimes=[],models=[],update_tool_state=Mock(),download_cancel=threading.Event(),set_status=Mock(),events=SimpleNamespace(installed=Mock(),progress=Mock(),catalog=Mock()),store=SimpleNamespace(home=Path(home)),launch=lambda work:work(),download_bar=Mock(),devices=Mock(currentData=Mock(return_value='Vulkan0')))
  runtime={'backend':'vulkan'};model={'label':'Q8_0','projector':{'label':'encoder'}}
  with patch('desktop.catalogs',return_value=([runtime],[model])) as catalog,patch('desktop.install_runtime',return_value=str(home)+'/server.exe') as install,patch('desktop.install_model',side_effect=DownloadError('Hugging Face rate limit reached')):
   Desktop.setup_all(owner)
   assert catalog.call_args.args[1]['github_token']=='test-github'
   assert install.call_args.args[0]['backend']=='vulkan'
   assert install.call_args.args[-1]['hf_token']=='test-hf'
   calls=owner.events.installed.emit.call_args_list
   assert calls[0].args[0]=='tools' and json.loads(calls[0].args[1])['server'].endswith('server.exe')
   assert calls[1].args==('setup','','Hugging Face rate limit reached')
  owner.installing=False;owner.events.installed.reset_mock()
  with patch('desktop.catalogs',return_value=([runtime],[model])),patch('desktop.install_model',return_value=str(home)+'/model-Q8_0.gguf') as install:
   Desktop.install(owner,'model')
   assert owner.events.installed.emit.call_args.args[-1]==''
   assert install.call_count==2 and install.call_args.args[-1]['hf_token']=='test-hf'
if __name__=='__main__':unittest.main()
