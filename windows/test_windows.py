import tempfile
import unittest
from unittest.mock import patch
import io
import urllib.error
from pathlib import Path
from vectordash_core import safe, read_text, cosine, command, embed, embedding_request, InputTooLarge

class WindowsTests(unittest.TestCase):
    def test_exclusions(self):
        for name in ['C:/Users/me/.ssh/id_rsa', 'C:/Windows/file.txt', 'C:/files/passwords.txt', 'C:/files/key.pem']:
            self.assertFalse(safe(Path(name)))
        self.assertTrue(safe(Path('C:/Users/me/Documents/notes.txt')))

    def test_cpu_is_explicit(self):
        args = command({'server': 'server.exe', 'model': 'model.gguf', 'mode': 'CPU / system RAM'})
        self.assertEqual(args[-4:], ['--device', 'none', '--gpu-layers', '0'])

    def test_gpu_requires_device(self):
        with self.assertRaises(ValueError):
            command({'server': 'server.exe', 'model': 'model.gguf', 'mode': 'Integrated GPU'})
        args = command({'server': 'server.exe', 'model': 'model.gguf', 'mode': 'Integrated GPU', 'device': 'Vulkan1'})
        self.assertEqual(args[-4:], ['--device', 'Vulkan1', '--gpu-layers', 'all'])

    def test_server_batch_sizes_match(self):
        args = command({'server': 'server.exe', 'model': 'model.gguf', 'mode': 'CPU / system RAM'})
        for flag in ('--batch-size', '--ubatch-size', '-c'):
            self.assertEqual(args[args.index(flag)+1], '2048')
        self.assertEqual(args[args.index('--parallel')+1], '1')

    def test_oversized_inputs_split_without_content_loss(self):
        accepted = []
        def runtime(text):
            if len(text) > 512:
                raise InputTooLarge('input too large; physical batch size 512')
            accepted.append(text)
            return [1.0, 0.0]
        text = '\u65e5\u672c\u8a9e sample ' * 2000
        with patch('vectordash_core.embedding_request', side_effect=runtime):
            self.assertEqual(embed(text), [1.0, 0.0])
        self.assertEqual(sum(map(len, accepted)), len(text))
        self.assertTrue(all(len(chunk) <= 512 for chunk in accepted))

    def test_unrelated_server_error_is_not_retried(self):
        error = urllib.error.HTTPError('local', 500, 'error', {}, io.BytesIO(b'{"error":"device out of memory"}'))
        with patch('vectordash_core.urllib.request.urlopen', side_effect=error) as request:
            with self.assertRaisesRegex(RuntimeError, 'out of memory'):
                embed('sample')
            self.assertEqual(request.call_count, 1)

    def test_size_error_body_is_recognized(self):
        error = urllib.error.HTTPError('local', 500, 'error', {}, io.BytesIO(b'{"error":"input (1390 tokens) is too large to process. increase the physical batch size (current batch size: 512)"}'))
        with patch('vectordash_core.urllib.request.urlopen', side_effect=error):
            with self.assertRaises(InputTooLarge):
                embedding_request('sample')

    def test_vectors(self):
        self.assertAlmostEqual(cosine([1, 0], [1, 0]), 1)
        self.assertEqual(cosine([1], [1, 0]), 0)
        self.assertEqual(cosine([0], [1]), 0)

    def test_text_and_size_limit(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'notes.txt'
            path.write_text('Windows search example', encoding='utf-8')
            self.assertEqual(read_text(path), 'Windows search example')
            with path.open('wb') as stream:
                stream.truncate(9 * 1024 * 1024)
            self.assertEqual(read_text(path), '')

if __name__ == '__main__':
    unittest.main()
