import unittest
from pathlib import Path
from windows_integration import startup_command

class StartupTests(unittest.TestCase):
    def test_installed_executable_starts_without_source_script(self):
        self.assertEqual(startup_command(r'C:\Program Files\VectorDash\VectorDash.exe',True),[r'C:\Program Files\VectorDash\VectorDash.exe','--startup'])
    def test_source_uses_windowless_python_and_script(self):
        command=startup_command(r'C:\Python314\python.exe',False)
        self.assertEqual(command[0],r'C:\Python314\pythonw.exe')
        self.assertEqual(Path(command[1]).name,'desktop.py')
        self.assertEqual(command[2],'--startup')
