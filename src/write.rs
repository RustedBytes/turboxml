use pyo3::prelude::*;

use crate::entities::{Node, RawNode};
use std::fs::File;
use std::io::{self, BufWriter, Write};

const XML_DECL: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n";

const TEXT_SPECIAL: [char; 5] = ['&', '<', '>', '"', '\''];
const ATTR_SPECIAL: [char; 8] = ['&', '<', '>', '"', '\'', '\t', '\r', '\n'];

/// Writes `value` with XML special characters escaped. Attribute values also
/// escape tab/CR/LF as character references — a spec-compliant reader
/// normalizes their literal forms to spaces (XML 1.0 §3.3.3), which would
/// corrupt round-trips.
fn write_escaped<W: Write>(out: &mut W, value: &str, attr: bool) -> io::Result<()> {
    let special: &[char] = if attr { &ATTR_SPECIAL } else { &TEXT_SPECIAL };
    let mut rest = value;
    while let Some(i) = rest.find(special) {
        out.write_all(&rest.as_bytes()[..i])?;
        let escaped: &[u8] = match rest.as_bytes()[i] {
            b'&' => b"&amp;",
            b'<' => b"&lt;",
            b'>' => b"&gt;",
            b'"' => b"&quot;",
            b'\'' => b"&apos;",
            b'\t' => b"&#9;",
            b'\r' => b"&#13;",
            _ => b"&#10;",
        };
        out.write_all(escaped)?;
        rest = &rest[i + 1..];
    }
    out.write_all(rest.as_bytes())
}

/// Pretty-printing emitter: every tag starts on a new, indented line, except
/// the first one and any tag that directly follows text, so text content
/// stays inline (`<a>text</a>`).
struct Emitter<W: Write> {
    out: W,
    indent: usize,
    level: usize,
    line_break: bool,
}

impl<W: Write> Emitter<W> {
    fn new(out: W, indent: usize) -> Self {
        Emitter {
            out,
            indent,
            level: 0,
            line_break: false,
        }
    }

    fn break_line(&mut self) -> io::Result<()> {
        if self.line_break {
            self.out.write_all(b"\n")?;
            for _ in 0..self.level * self.indent {
                self.out.write_all(b" ")?;
            }
        }
        self.line_break = true;
        Ok(())
    }

    fn write_node(&mut self, node: RawNode) -> io::Result<()> {
        self.break_line()?;
        self.out.write_all(b"<")?;
        self.out.write_all(node.name.as_bytes())?;
        for (k, v) in &node.attrs {
            self.out.write_all(b" ")?;
            self.out.write_all(k.as_bytes())?;
            self.out.write_all(b"=\"")?;
            write_escaped(&mut self.out, v, true)?;
            self.out.write_all(b"\"")?;
        }
        if node.children.is_empty() && node.text.is_none() {
            return self.out.write_all(b"/>");
        }
        self.out.write_all(b">")?;
        self.level += 1;
        if let Some(text) = node.text {
            write_escaped(&mut self.out, &text, false)?;
            self.line_break = false;
        }
        for child in node.children {
            self.write_node(child)?;
        }
        self.level -= 1;
        self.break_line()?;
        self.out.write_all(b"</")?;
        self.out.write_all(node.name.as_bytes())?;
        self.out.write_all(b">")
    }
}

pub fn write_node_to_string(
    node: RawNode,
    indent: usize,
    default_xml_def: bool,
) -> io::Result<String> {
    let mut buf = Vec::new();
    if default_xml_def {
        buf.extend_from_slice(XML_DECL.as_bytes());
    }
    let mut emitter = Emitter::new(buf, indent);
    emitter.write_node(node)?;
    Ok(String::from_utf8(emitter.out).expect("invariant: the XML writer only emits valid UTF-8"))
}

#[pyfunction]
#[pyo3(signature = (node, file_path, indent=None, default_xml_def=None))]
pub fn write_file(
    py: Python<'_>,
    node: PyRef<'_, Node>,
    file_path: String,
    indent: Option<usize>,
    default_xml_def: Option<bool>,
) -> PyResult<()> {
    let _indent = indent.unwrap_or(4);
    let _default_xml_def = default_xml_def.unwrap_or(true);
    let raw = node.to_raw(py);
    py.detach(|| -> PyResult<()> {
        let file = File::create(&file_path)?;
        let mut buf = BufWriter::new(file);
        if _default_xml_def {
            buf.write_all(XML_DECL.as_bytes())?;
        }
        let mut emitter = Emitter::new(buf, _indent);
        emitter.write_node(raw)?;
        emitter.out.flush()?;
        Ok(())
    })
}

#[pyfunction]
#[pyo3(signature = (node, indent=None, default_xml_def=None))]
pub fn write_string(
    py: Python<'_>,
    node: PyRef<'_, Node>,
    indent: Option<usize>,
    default_xml_def: Option<bool>,
) -> PyResult<String> {
    let _indent = indent.unwrap_or(4);
    let _default_xml_def = default_xml_def.unwrap_or(true);
    let raw = node.to_raw(py);
    py.detach(|| Ok(write_node_to_string(raw, _indent, _default_xml_def)?))
}

#[cfg(test)]
mod tests {
    use crate::entities::{Node, RawNode};
    use crate::f_str;
    use crate::read::read_string;
    use crate::write::{write_file, write_node_to_string, write_string};
    use pyo3::Python;
    use std::collections::HashMap;
    use std::fs::{read_to_string, remove_file};
    fn root_node() -> RawNode {
        let mut attrs = HashMap::new();
        attrs.insert(f_str!("test").into(), f_str!("test").into());
        let mut root = RawNode {
            name: f_str!("root").into(),
            attrs: attrs.clone(),
            children: Vec::new(),
            text: None,
        };
        let mut child = RawNode {
            name: f_str!("child").into(),
            attrs: attrs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            children: Vec::new(),
            text: None,
        };
        child.children.push(RawNode {
            name: f_str!("child").into(),
            attrs: HashMap::new(),
            children: Vec::new(),
            text: Some(f_str!("test").into()),
        });
        root.children.push(child);
        root
    }
    fn expected_file() -> &'static str {
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<root test=\"test\">\n    <child test=\"test\">\n        <child>test</child>\n    </child>\n</root>"
    }
    #[test]
    fn test_write_node_to_string() {
        let root = root_node();
        let expected = expected_file();
        let result = write_node_to_string(root, 4, true).unwrap();
        assert_eq!(result, expected);
    }
    #[test]
    fn test_write_string() {
        let expected = expected_file();
        Python::initialize();
        let result = Python::attach(|py| {
            let node = Node::from_raw(py, root_node()).unwrap();
            write_string(py, node.borrow(py), Some(4), Some(true)).unwrap()
        });
        assert_eq!(result, expected);
    }
    #[test]
    fn test_write_file() {
        let expected = expected_file();
        Python::initialize();
        Python::attach(|py| {
            let node = Node::from_raw(py, root_node()).unwrap();
            write_file(
                py,
                node.borrow(py),
                f_str!("tests/test_write.xml"),
                Some(4),
                Some(true),
            )
            .unwrap();
        });
        let file_str = read_to_string("tests/test_write.xml").unwrap();
        remove_file("tests/test_write.xml").unwrap();
        assert_eq!(file_str, expected);
    }
    #[test]
    fn test_write_self_closing_tag() {
        let mut root = RawNode {
            name: f_str!("root").into(),
            attrs: HashMap::new(),
            children: Vec::new(),
            text: None,
        };
        let mut attrs = HashMap::new();
        attrs.insert(f_str!("attr"), f_str!("value"));
        root.children.push(RawNode {
            name: f_str!("child").into(),
            attrs: attrs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            children: Vec::new(),
            text: None,
        });
        let expected = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<root>\n    <child attr=\"value\"/>\n</root>";
        Python::initialize();
        let result = Python::attach(|py| {
            let node = Node::from_raw(py, root).unwrap();
            write_string(py, node.borrow(py), Some(4), Some(true)).unwrap()
        });
        assert_eq!(result, expected);
    }
    #[test]
    fn test_roundtrip_preserves_special_characters() {
        let mut attrs = HashMap::new();
        attrs.insert(f_str!("attr"), f_str!("a & b"));
        let root = RawNode {
            name: f_str!("root").into(),
            attrs: attrs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            children: Vec::new(),
            text: Some(f_str!("1 < 2 & 3").into()),
        };
        Python::initialize();
        Python::attach(|py| {
            let node = Node::from_raw(py, root).unwrap();
            let xml = write_string(py, node.borrow(py), Some(4), Some(true)).unwrap();
            assert!(xml.contains("a &amp; b"));
            assert!(xml.contains("1 &lt; 2 &amp; 3"));
            let reread = read_string(
                py,
                pyo3::types::PyString::new(py, &xml).try_into().unwrap(),
                f_str!("root"),
            )
            .unwrap();
            let reread = reread.borrow(py);
            assert_eq!(reread.attrs.get("attr").unwrap(), "a & b");
            assert_eq!(reread.text.as_ref().unwrap(), "1 < 2 & 3");
        });
    }
    #[test]
    fn test_roundtrip_preserves_attr_whitespace() {
        let mut attrs = HashMap::new();
        attrs.insert(f_str!("attr"), f_str!("line1\nline2\tend"));
        let root = RawNode {
            name: f_str!("root").into(),
            attrs: attrs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            children: Vec::new(),
            text: None,
        };
        Python::initialize();
        Python::attach(|py| {
            let node = Node::from_raw(py, root).unwrap();
            let xml = write_string(py, node.borrow(py), Some(4), Some(true)).unwrap();
            assert!(xml.contains("&#10;"));
            assert!(xml.contains("&#9;"));
            let reread = read_string(
                py,
                pyo3::types::PyString::new(py, &xml).try_into().unwrap(),
                f_str!("root"),
            )
            .unwrap();
            let reread = reread.borrow(py);
            assert_eq!(reread.attrs.get("attr").unwrap(), "line1\nline2\tend");
        });
    }
}
