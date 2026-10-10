"""Tool setup integrity, input validation and Windows installer regression checks."""
import hashlib
import io
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import patch,Mock
from setup_runtime import download
from setup_tools import install_ocr,install_ffmpeg,language_codes

ROOT=Path(__file__).resolve().parents[1]
class Response(io.BytesIO):
    status=200
    headers={}

class ToolTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(dir=ROOT,prefix='vectordash-tools-')
        self.addCleanup(self.temp.cleanup)
        self.home=Path(self.temp.name)
        self.cancel=threading.Event()

    def test_language_codes_reject_path_traversal_before_setup(self):
        for value in ('../eng','eng+../../file','eng+','eng;command'):
            with self.assertRaises(ValueError):
                language_codes(value)
        self.assertEqual(language_codes('eng+ben+ara+eng'),['eng','ben','ara'])

    def test_git_blob_hash_verified_before_replacing_language(self):
        content=b'valid official language data'
        digest=hashlib.sha1(b'blob '+str(len(content)).encode()+b'\0'+content).hexdigest()
        target=self.home/'eng.traineddata'
        target.write_bytes(b'previous language')
        with patch('setup_runtime.urllib.request.urlopen',return_value=Response(content)):
            download('https://official/file',target,len(content),'git-sha1:'+digest,self.cancel,lambda *args:None)
        self.assertEqual(target.read_bytes(),content)
        with patch('setup_runtime.urllib.request.urlopen',return_value=Response(b'corrupt data')):
            with self.assertRaisesRegex(RuntimeError,'Checksum'):
                download('https://official/file',target,len(b'corrupt data'),'git-sha1:'+digest,self.cancel,lambda *args:None)
        self.assertEqual(target.read_bytes(),content)

    def test_cancelled_setup_never_starts_installer(self):
        self.cancel.set()
        with patch('setup_tools.subprocess.Popen') as process:
            with self.assertRaises(InterruptedError):
                install_ocr({'ocr_languages':'eng'},self.home,self.cancel,lambda *args:None)
            process.assert_not_called()
        with self.assertRaises(InterruptedError):
            install_ffmpeg(self.home,self.cancel,lambda *args:None)

    def test_windows_install_then_pinned_language_setup(self):
        program='C:/Programs/Tesseract-OCR/tesseract.exe'
        process=Mock(returncode=0)
        data={'tree':[{'type':'blob','path':'eng.traineddata','size':10,'sha':'abc123'}]}
        with patch('setup_tools.ocr_program',side_effect=[None,None,program]),patch('setup_tools.shutil.which',return_value='winget.exe'),patch('setup_tools.subprocess.Popen',return_value=process) as launch,patch('setup_tools.check_program'),patch('setup_tools.fetch_json',side_effect=[{'sha':'pinned-revision'},data]),patch('setup_tools.download') as get,patch('setup_tools.process_lifecycle.run',return_value=Mock(returncode=0,stdout='List of languages:\neng\n')):
            result=install_ocr({'ocr_languages':'eng'},self.home,self.cancel,lambda *args:None)
        args=launch.call_args.args[0]
        self.assertIn('UB-Mannheim.TesseractOCR',args)
        self.assertEqual(args[-1],'/S /CurrentUser')
        process.wait.assert_called_once()
        self.assertIn('/pinned-revision/eng.traineddata',get.call_args.args[0])
        self.assertEqual(get.call_args.args[3],'git-sha1:abc123')
        self.assertEqual(result['ocr_path'],program)
        self.assertEqual(result['ocr_data'],str(self.home/'tools/tessdata'))

    def test_ocr_managed_path_with_spaces_and_exact_tsv_text(self):
        from PIL import Image
        from content import ocr
        header='level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n'
        row='5\t1\t1\t1\t1\t1\t20\t30\t80\t15\t96\tINV-2026_8832\n'
        data=(header+row).encode()
        settings={'ocr_path':'C:/OCR/tesseract.exe','ocr_data':'C:/Users/A User/OCR data','ocr_languages':'eng'}
        with patch('content.ocr_program',return_value=settings['ocr_path']),patch('content.process_lifecycle.run',return_value=Mock(returncode=0,stdout=data)) as run:
            text,boxes=ocr(Image.new('RGB',(100,100),'white'),settings)
        self.assertEqual(text,'INV-2026_8832')
        self.assertEqual(boxes,[['INV-2026_8832',20,30,80,15]])
        args=run.call_args.args[0]
        self.assertEqual(args[args.index('--tessdata-dir')+1],str(Path(settings['ocr_data'])))
        self.assertTrue(run.call_args.kwargs['input'].startswith(b'\x89PNG'))

    def test_missing_winget_reports_actionable_error(self):
        with patch('setup_tools.ocr_program',return_value=None),patch('setup_tools.shutil.which',return_value=None):
            with self.assertRaisesRegex(RuntimeError,'App Installer'):
                install_ocr({'ocr_languages':'eng'},self.home,self.cancel,lambda *args:None)

    def test_invalid_manual_path_reuses_detected_existing_ocr(self):
        program='C:/Programs/tesseract.exe'
        with patch('setup_tools.ocr_program',side_effect=[None,program]),patch('setup_tools.check_program'),patch('setup_tools.fetch_json',side_effect=[{'sha':'rev'},{'tree':[{'type':'blob','path':'eng.traineddata','size':1,'sha':'abc'}]}]),patch('setup_tools.download'),patch('setup_tools.process_lifecycle.run',return_value=Mock(returncode=0,stdout='eng\n')),patch('setup_tools.subprocess.Popen') as launch:
            result=install_ocr({'ocr_path':'invalid.exe','ocr_languages':'eng'},self.home,self.cancel,lambda *args:None)
        launch.assert_not_called()
        self.assertEqual(result['ocr_path'],program)

if __name__=='__main__':
    unittest.main()
