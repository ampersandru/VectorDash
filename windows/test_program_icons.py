import os
import unittest
from unittest.mock import patch
from PySide6.QtWidgets import QApplication
from PySide6.QtGui import QImage
from previews import Preview
from engine import SearchResult
from program_icons import load_icon
from thumbnails import Thumbnails

class ProgramIconTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.app=QApplication.instance() or QApplication([])

    @unittest.skipUnless(os.name=='nt','Windows icon extraction')
    def test_native_shell_icon_and_invalid_app_id(self):
        image=load_icon(os.path.join(os.environ['SystemRoot'],'explorer.exe'))
        self.assertIsNotNone(image);self.assertFalse(image.isNull())
        self.assertEqual(image.width(),64)
        self.assertTrue(any(image.pixelColor(x,y).alpha() for x in range(64) for y in range(64)))
        self.assertIsNone(load_icon('shell:AppsFolder\\invalid;id'))

    def test_program_preview_receives_cached_icon(self):
        image=QImage(64,64,QImage.Format.Format_ARGB32);image.fill(0xff8855cc)
        result=SearchResult(3,'C:/Example.lnk','Start menu',kind='application',symbol='Example')
        with patch.object(Thumbnails,'request',return_value=image):
            preview=Preview(result,embedded=True)
            self.assertFalse(preview.program_icon.pixmap().isNull())
            self.assertEqual(preview.program_icon.pixmap().width(),64)
            preview.close()

if __name__=='__main__':unittest.main()
