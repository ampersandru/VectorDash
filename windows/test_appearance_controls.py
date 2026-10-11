"""Appearance settings and model severity parsing regressions."""
import unittest
from pathlib import Path
import tempfile
from appearance import popup_colors,color
from log_support import log_level,tail

class AppearanceTests(unittest.TestCase):
    def test_transparency_and_gradient_controls(self):
        first,last=popup_colors({'popup_transparency':0,'popup_gradient':0},True)
        self.assertEqual(first.alpha(),255);self.assertEqual(first,last)
        first,last=popup_colors({'popup_transparency':60,'popup_gradient':100},True)
        self.assertEqual(first.alpha(),102);self.assertNotEqual(first,last)
        first,last=popup_colors({'popup_transparency':999,'popup_gradient':-50},False)
        self.assertEqual(first,last);self.assertEqual(first.alpha(),26)
    def test_any_color_tone_and_invalid_fallback(self):
        green,_=popup_colors({'popup_tone':'#22aa66'},True)
        self.assertGreater(green.green(),green.red())
        self.assertEqual(color('invalid').name(),'#8573ff')
    def test_model_severity_formats(self):
        for line,level in [('0.00.090.052 W srv warning','WARNING'),('2.25.243.234 E srv failure','ERROR'),('2026-10-10 ERROR problem','ERROR'),('I model loaded','INFO'),('D diagnostics','DEBUG'),('normal output','INFO')]:self.assertEqual(log_level(line),level)
    def test_model_tail_is_bounded(self):
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'model.log';path.write_bytes(b'old log\n'*100+b'I final entry\n')
            lines=tail(path,80)
            self.assertEqual(lines[-1],'I final entry');self.assertLess(len('\n'.join(lines)),80)

if __name__=='__main__':unittest.main()
