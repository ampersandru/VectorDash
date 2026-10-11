import io
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import Mock,patch
from desktop import Desktop
from engine import DEFAULTS
from vectordash_core import command

class StartupTests(unittest.TestCase):
    def test_existing_text_server_restarts_with_encoder(self):
        with tempfile.TemporaryDirectory() as temporary:
            home=Path(temporary)
            for name in ('server.exe','model.gguf','mmproj.gguf'):(home/name).touch()
            settings=dict(DEFAULTS,server=str(home/'server.exe'),model=str(home/'model.gguf'),projector=str(home/'mmproj.gguf'),mode='CPU / system RAM')
            window=SimpleNamespace(settings=settings,indexing=False,save=lambda **kw:True,model_process=None,attached_server=None,store=SimpleNamespace(home=home),model_state=Mock(),set_status=Mock(),error=Mock(),launch=Mock(),probe_model=Mock(),model_status=Mock())
            window.attach_local_server=lambda:setattr(window,'attached_server',Mock())
            stopped=[]
            def stop():
                stopped.append(True);window.attached_server=None
            window.stop_model=stop
            response=io.BytesIO(json.dumps({'modalities':{'vision':False,'audio':False}}).encode())
            with patch('desktop.net_socket.socket') as socket,patch('desktop.verify_server_model'),patch('desktop.urllib.request.urlopen',return_value=response),patch('desktop.process_lifecycle.popen') as launch:
                socket.return_value.__enter__.return_value.connect_ex.return_value=0
                Desktop.start_model(window)
                window.error.assert_not_called()
                self.assertTrue(stopped)
                self.assertIn('--mmproj',launch.call_args.args[0])
                self.assertIn(str(home/'mmproj.gguf'),launch.call_args.args[0])
                window.launch.assert_called_once()
            window.model_log.close()

    def test_existing_server_restarts_for_high_detail(self):
        with tempfile.TemporaryDirectory() as temporary:
            home=Path(temporary)
            for name in ('server.exe','model.gguf','mmproj.gguf'):(home/name).touch()
            settings=dict(DEFAULTS,server=str(home/'server.exe'),model=str(home/'model.gguf'),projector=str(home/'mmproj.gguf'),visual_detail='High detail')
            window=SimpleNamespace(settings=settings,indexing=False,save=lambda **kw:True,model_process=None,attached_server=None,store=SimpleNamespace(home=home),model_state=Mock(),set_status=Mock(),error=Mock(),launch=Mock(),probe_model=Mock(),model_status=Mock())
            process=Mock();process.cmdline.return_value=command(dict(settings,visual_detail='Balanced'))
            window.attach_local_server=lambda:setattr(window,'attached_server',process)
            stopped=[]
            def stop():
                stopped.append(True);window.attached_server=None
            window.stop_model=stop
            response=io.BytesIO(json.dumps({'modalities':{'vision':True,'audio':True}}).encode())
            with patch('desktop.net_socket.socket') as socket,patch('desktop.verify_server_model'),patch('desktop.urllib.request.urlopen',return_value=response),patch('desktop.process_lifecycle.popen') as launch:
                socket.return_value.__enter__.return_value.connect_ex.return_value=0
                Desktop.start_model(window)
                window.error.assert_not_called();self.assertTrue(stopped)
                args=launch.call_args.args[0]
                self.assertEqual(args[args.index('--image-max-tokens')+1],'1120')
            window.model_log.close()
