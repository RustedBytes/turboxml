"""Regression coverage for Python-owned input storage while parsing off the GIL."""
import unittest
from concurrent.futures import ThreadPoolExecutor

import turboxml


class ReadStringTests(unittest.TestCase):
    def test_temporary_unicode_input_in_threads(self):
        def parse(i):
            # No caller-owned persistent XML buffer; each task has different text.
            root = turboxml.read_string(
                '<root id="%d">%s</root>' % (i, 'Україна ' * 1024), 'root'
            )
            return root.attrs['id'], root.text

        with ThreadPoolExecutor(max_workers=8) as pool:
            for i, (identifier, text) in enumerate(pool.map(parse, range(64))):
                self.assertEqual(identifier, str(i))
                self.assertEqual(text, ('Україна ' * 1024).strip())

    def test_text_segments_and_entities(self):
        root = turboxml.read_string(
            '<root>a<![CDATA[b]]>c&amp;d<child/>e</root>', 'root'
        )
        self.assertEqual(root.text, 'abc&de')
        self.assertEqual(root.children[0].name, 'child')


if __name__ == '__main__':
    unittest.main()
