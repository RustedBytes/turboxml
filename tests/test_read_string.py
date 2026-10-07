"""Regression coverage for Python-owned input storage while parsing off the GIL."""
import unittest
import gc
import os
import tempfile
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


    def test_child_retains_document_after_root_and_input_are_dropped(self):
        xml = '<root><child key="Україна"> значення </child></root>'
        root = turboxml.read_string(xml, 'root')
        child = root.children[0]
        del xml, root
        gc.collect()
        self.assertEqual(child.name, 'child')
        self.assertEqual(child.attrs, {'key': 'Україна'})
        self.assertEqual(child.text, 'значення')
        snapshot = child.attrs
        snapshot['key'] = 'changed'
        self.assertEqual(child.attrs['key'], 'Україна')
        self.assertEqual(child.to_dict()['text'], 'значення')
        self.assertIn('значення', turboxml.write_string(child))
        copied = turboxml.Node.from_dict(child.to_dict())
        del child
        gc.collect()
        self.assertEqual(copied.text, 'значення')
        self.assertEqual(copied.attrs, {'key': 'Україна'})

    def test_file_buffer_survives_return_and_file_removal(self):
        with tempfile.TemporaryDirectory() as directory:
            path = os.path.join(directory, 'document.xml')
            with open(path, 'w', encoding='utf-8') as file:
                file.write('<root a="value"><child>Україна</child></root>')
            root = turboxml.read_file(path, 'root')
            child = root.children[0]
            os.remove(path)
        del root
        gc.collect()
        self.assertEqual(child.name, 'child')
        self.assertEqual(child.text, 'Україна')


if __name__ == '__main__':
    unittest.main()
