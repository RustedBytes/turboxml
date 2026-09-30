use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use simdxml::index::TagType;
use simdxml::{SimdXmlError, XmlIndex};
use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt;
use std::fs;
use std::panic::{self, AssertUnwindSafe};

use crate::entities::{Node, RawNode};

#[derive(Debug)]
pub enum ParseError {
    Index(SimdXmlError),
    Syntax { position: usize, message: String },
    RootTagNotFound { root_tag: String },
    UnexpectedEof { tag: String },
    UnknownEntity { position: usize, name: String },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Index(source) => write!(f, "malformed XML: {source}"),
            ParseError::Syntax { position, message } => {
                write!(f, "malformed XML at byte {position}: {message}")
            }
            ParseError::RootTagNotFound { root_tag } => {
                write!(f, "root tag <{root_tag}> not found in document")
            }
            ParseError::UnexpectedEof { tag } => {
                write!(f, "unexpected end of document: <{tag}> is never closed")
            }
            ParseError::UnknownEntity { position, name } => {
                write!(f, "unknown entity reference '&{name};' at byte {position}")
            }
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParseError::Index(source) => Some(source),
            ParseError::Syntax { .. }
            | ParseError::RootTagNotFound { .. }
            | ParseError::UnexpectedEof { .. }
            | ParseError::UnknownEntity { .. } => None,
        }
    }
}

impl From<ParseError> for PyErr {
    fn from(error: ParseError) -> Self {
        PyValueError::new_err(error.to_string())
    }
}

fn syntax(position: usize, message: impl Into<String>) -> ParseError {
    ParseError::Syntax {
        position,
        message: message.into(),
    }
}

/// Resolves entity and character references in `raw`, which starts at byte
/// `offset` of the document. With `normalize_ws`, literal tab/CR/LF become
/// spaces first, as XML 1.0 §3.3.3 requires for attribute values.
fn unescape(raw: &str, offset: usize, normalize_ws: bool) -> Result<Cow<'_, str>, ParseError> {
    let needs_ws = normalize_ws && raw.contains(['\t', '\r', '\n']);
    if !needs_ws && !raw.contains('&') {
        return Ok(Cow::Borrowed(raw));
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(i) = rest.find(['&', '\t', '\r', '\n']) {
        let (plain, tail) = rest.split_at(i);
        out.push_str(plain);
        let position = offset + (raw.len() - tail.len());
        match tail.as_bytes()[0] {
            b'&' => {
                let end = tail
                    .find(';')
                    .ok_or_else(|| syntax(position, "unterminated entity reference"))?;
                let name = &tail[1..end];
                out.push(resolve_entity(name, position)?);
                rest = &tail[end + 1..];
            }
            ws if normalize_ws => {
                out.push(' ');
                // A CRLF pair is a single line break (XML 1.0 §2.11).
                let skip = if ws == b'\r' && tail.as_bytes().get(1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                rest = &tail[skip..];
            }
            ws => {
                out.push(ws as char);
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    Ok(Cow::Owned(out))
}

fn resolve_entity(name: &str, position: usize) -> Result<char, ParseError> {
    let ch = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => {
            let Some(num) = name.strip_prefix('#') else {
                return Err(ParseError::UnknownEntity {
                    position,
                    name: name.to_string(),
                });
            };
            let code = match num.strip_prefix('x') {
                Some(hex) => u32::from_str_radix(hex, 16),
                None => num.parse::<u32>(),
            };
            return code
                .ok()
                .filter(|&c| c != 0)
                .and_then(char::from_u32)
                .ok_or_else(|| {
                    syntax(position, format!("invalid character reference '&{name};'"))
                });
        }
    };
    Ok(ch)
}

/// Parses the attributes of a start tag. `body` is the text after the tag
/// name up to (not including) the closing `>` or `/>`, and starts at byte
/// `offset` of the document.
fn parse_attrs(body: &str, offset: usize) -> Result<HashMap<String, String>, ParseError> {
    let mut map = HashMap::new();
    let bytes = body.as_bytes();
    let mut pos = 0;
    loop {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos == bytes.len() {
            return Ok(map);
        }
        if pos == start {
            return Err(syntax(offset + pos, "expected whitespace before attribute"));
        }
        let key_start = pos;
        while pos < bytes.len() && !matches!(bytes[pos], b'=' | b'"' | b'\'') {
            if bytes[pos].is_ascii_whitespace() {
                break;
            }
            pos += 1;
        }
        let key = &body[key_start..pos];
        if key.is_empty() {
            return Err(syntax(offset + key_start, "expected attribute name"));
        }
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if bytes.get(pos) != Some(&b'=') {
            return Err(syntax(
                offset + key_start,
                format!("attribute '{key}' has no value"),
            ));
        }
        pos += 1;
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let quote = match bytes.get(pos) {
            Some(&q @ (b'"' | b'\'')) => q,
            _ => {
                return Err(syntax(
                    offset + pos,
                    format!("value of attribute '{key}' is not quoted"),
                ));
            }
        };
        pos += 1;
        let value_start = pos;
        let len = body[value_start..]
            .bytes()
            .position(|b| b == quote)
            .ok_or_else(|| syntax(offset + value_start, "unterminated attribute value"))?;
        pos = value_start + len;
        let raw = &body[value_start..pos];
        if let Some(i) = raw.find('<') {
            return Err(syntax(
                offset + value_start + i,
                "'<' is not allowed in attribute values",
            ));
        }
        let value = unescape(raw, offset + value_start, true)?.into_owned();
        match map.entry(key.to_string()) {
            Entry::Occupied(_) => {
                return Err(syntax(
                    offset + key_start,
                    format!("duplicate attribute '{key}'"),
                ));
            }
            Entry::Vacant(slot) => {
                slot.insert(value);
            }
        }
        pos += 1;
    }
}

/// Validates an element tag at index `i` and returns its name and the byte
/// range of its attribute section.
fn element_parts<'a>(
    xml: &'a str,
    index: &XmlIndex<'a>,
    i: usize,
) -> Result<(&'a str, &'a str, usize), ParseError> {
    let start = index.tag_starts[i] as usize;
    let end = index.tag_ends[i] as usize;
    let name = index.tag_name(i);
    if name.is_empty() {
        return Err(syntax(start, "expected element name after '<'"));
    }
    let body_start = start + 1 + name.len();
    let body_end = match index.tag_type(i) {
        TagType::SelfClose => end - 1,
        _ => end,
    };
    Ok((name, &xml[body_start..body_end], body_start))
}

fn node_from_tag(xml: &str, index: &XmlIndex<'_>, i: usize) -> Result<RawNode, ParseError> {
    let (name, body, offset) = element_parts(xml, index, i)?;
    Ok(RawNode {
        name: name.to_string(),
        attrs: parse_attrs(body, offset)?,
        children: Vec::new(),
        text: None,
    })
}

/// An element under construction. Text is accumulated across text segments,
/// entity references and CDATA sections, and trimmed once on close.
struct Frame {
    node: RawNode,
    text: String,
}

impl Frame {
    fn finish(self) -> RawNode {
        let mut node = self.node;
        let text = self.text.trim();
        if !text.is_empty() {
            node.text = Some(text.to_string());
        }
        node
    }
}

/// Builds the structural index. simdxml panics on a few truncated inputs
/// (e.g. an unterminated comment), so those are reported as parse errors.
fn build_index(xml: &str) -> Result<XmlIndex<'_>, ParseError> {
    panic::catch_unwind(AssertUnwindSafe(|| simdxml::parse(xml.as_bytes())))
        .map_err(|_| syntax(xml.len(), "unexpected end of document"))?
        .map_err(ParseError::Index)
}

/// Scans for the first element named `root_tag` — at any depth, so a subtree
/// of a larger document can be extracted — and parses it and its descendants.
///
/// simdxml produces a flat, document-ordered index of tags and text ranges
/// without checking well-formedness, so this walk validates nesting (matching
/// by depth, not tag name, keeps children that share an ancestor's name
/// intact), tag terminators and attribute syntax while building the tree.
fn parse_str(xml: &str, root_tag: &str) -> Result<RawNode, ParseError> {
    let index = build_index(xml)?;
    let ranges = &index.text_ranges;
    let mut next_text = 0;
    // Open elements outside the requested root; only names are needed.
    let mut outer: Vec<&str> = Vec::new();
    // Open elements from the requested root down; empty until it is found.
    let mut stack: Vec<Frame> = Vec::new();

    for i in 0..index.tag_count() {
        let start = index.tag_starts[i] as usize;
        let end = index.tag_ends[i] as usize;
        if end >= xml.len() {
            return Err(syntax(start, "unterminated markup"));
        }

        while next_text < ranges.len() && (ranges[next_text].start as usize) < start {
            let range = &ranges[next_text];
            next_text += 1;
            if let Some(frame) = stack.last_mut() {
                let raw = index.text_content(range);
                if let Some(j) = raw.find('<') {
                    return Err(syntax(range.start as usize + j, "unexpected '<'"));
                }
                frame
                    .text
                    .push_str(&unescape(raw, range.start as usize, false)?);
            }
        }

        match index.tag_type(i) {
            TagType::Open => {
                if stack.is_empty() && index.tag_name(i) != root_tag {
                    outer.push(element_parts(xml, &index, i)?.0);
                } else {
                    stack.push(Frame {
                        node: node_from_tag(xml, &index, i)?,
                        text: String::new(),
                    });
                }
            }
            TagType::SelfClose => {
                let node = node_from_tag(xml, &index, i)?;
                match stack.last_mut() {
                    Some(frame) => frame.node.children.push(node),
                    None if node.name == root_tag => return Ok(node),
                    None => (),
                }
            }
            TagType::Close => {
                let name = index.tag_name(i);
                let trailing = &xml[start + 2 + name.len()..end];
                if !trailing.trim_start().is_empty() {
                    return Err(syntax(start, format!("malformed end tag </{name}>")));
                }
                let expected = match stack.last() {
                    Some(frame) => Some(frame.node.name.as_str()),
                    None => outer.last().copied(),
                };
                if expected != Some(name) {
                    let message = match expected {
                        Some(open) => format!("expected </{open}>, found </{name}>"),
                        None => format!("unexpected end tag </{name}>"),
                    };
                    return Err(syntax(start, message));
                }
                match stack.pop() {
                    Some(frame) => {
                        let node = frame.finish();
                        match stack.last_mut() {
                            Some(parent) => parent.node.children.push(node),
                            None => return Ok(node),
                        }
                    }
                    None => {
                        outer.pop();
                    }
                }
            }
            TagType::CData => {
                if !xml[..=end].ends_with("]]>") {
                    return Err(syntax(start, "unterminated CDATA section"));
                }
                if let Some(frame) = stack.last_mut() {
                    frame.text.push_str(&xml[start + 9..end - 2]);
                }
                // The CDATA content is also indexed as a text range.
                while next_text < ranges.len() && (ranges[next_text].start as usize) < end {
                    next_text += 1;
                }
            }
            TagType::Comment => {
                // `end < start + 6` rejects `<!-->`, whose tail also reads `-->`.
                if end < start + 6 || !xml[..=end].ends_with("-->") {
                    return Err(syntax(start, "unterminated comment"));
                }
            }
            TagType::PI => {
                if !xml[..=end].ends_with("?>") {
                    return Err(syntax(start, "unterminated processing instruction"));
                }
            }
        }
    }

    match stack.pop() {
        Some(frame) => Err(ParseError::UnexpectedEof {
            tag: frame.node.name,
        }),
        None => Err(ParseError::RootTagNotFound {
            root_tag: root_tag.to_string(),
        }),
    }
}

#[pyfunction]
pub fn read_file(py: Python<'_>, file_path: String, root_tag: String) -> PyResult<Py<Node>> {
    let raw = py.detach(|| -> PyResult<RawNode> {
        let file_str = fs::read_to_string(&file_path)?;
        Ok(parse_str(&file_str, &root_tag)?)
    })?;
    Node::from_raw(py, raw)
}

#[pyfunction]
pub fn read_string(py: Python<'_>, xml_string: String, root_tag: String) -> PyResult<Py<Node>> {
    let raw = py.detach(|| parse_str(&xml_string, &root_tag))?;
    Node::from_raw(py, raw)
}

#[cfg(test)]
mod tests {
    use crate::f_str;
    use crate::read::{read_file, read_string};
    use pyo3::Python;
    use pyo3::exceptions::{PyFileNotFoundError, PyValueError};
    use std::fs::{File, remove_file};
    use std::io::prelude::*;
    #[test]
    fn test_read_file() {
        let mut file = File::create("tests/test.xml").unwrap();
        file.write_all(b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<root test=\"test\">\n    <child test=\"test\">\n        <child>test</child>\n    </child>\n</root>")
            .unwrap();
        Python::initialize();
        Python::attach(|py| {
            let handle = read_file(py, f_str!("tests/test.xml"), f_str!("root")).unwrap();
            remove_file("tests/test.xml").unwrap();
            let node = handle.borrow(py);
            assert_eq!(node.name, f_str!("root"));
            assert_eq!(node.attrs.len(), 1);
            assert_eq!(node.attrs.get("test").unwrap(), "test");
            assert_eq!(node.children.len(), 1);
            let child = node.children[0].borrow(py);
            assert_eq!(child.name, f_str!("child"));
            assert_eq!(child.attrs.len(), 1);
            assert_eq!(child.attrs.get("test").unwrap(), "test");
            // The nested <child> is a real child node; the old parser
            // collapsed same-named nesting and hoisted its text.
            assert_eq!(child.text, None);
            assert_eq!(child.children.len(), 1);
            let grandchild = child.children[0].borrow(py);
            assert_eq!(grandchild.text.as_ref().unwrap(), "test");
        });
    }
    #[test]
    fn test_read_self_closing_tag() {
        let xml_string = f_str!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?><tag><wrapper><inner1>value</inner1><inner2 attr=\"attr\"/><inner3>value</inner3></wrapper></tag>"
        );
        Python::initialize();
        Python::attach(|py| {
            let handle = read_string(py, xml_string, f_str!("tag")).unwrap();
            let node = handle.borrow(py);
            assert_eq!(node.children[0].borrow(py).children.len(), 3);
        });
    }
    #[test]
    fn test_read_unescapes_text_and_attrs() {
        let xml_string = f_str!("<root attr=\"a &amp; b\">1 &lt; 2 &amp; 3</root>");
        Python::initialize();
        Python::attach(|py| {
            let handle = read_string(py, xml_string, f_str!("root")).unwrap();
            let node = handle.borrow(py);
            assert_eq!(node.attrs.get("attr").unwrap(), "a & b");
            assert_eq!(node.text.as_ref().unwrap(), "1 < 2 & 3");
        });
    }
    #[test]
    fn test_read_child_with_same_name_as_root() {
        let xml_string =
            f_str!("<item outer=\"1\"><item inner=\"2\"><item>deep</item></item><other/></item>");
        Python::initialize();
        Python::attach(|py| {
            let handle = read_string(py, xml_string, f_str!("item")).unwrap();
            let root = handle.borrow(py);
            assert_eq!(root.attrs.get("outer").unwrap(), "1");
            assert_eq!(root.children.len(), 2);
            let inner = root.children[0].borrow(py);
            assert_eq!(inner.name, "item");
            assert_eq!(inner.attrs.get("inner").unwrap(), "2");
            assert_eq!(inner.children.len(), 1);
            assert_eq!(inner.children[0].borrow(py).text.as_ref().unwrap(), "deep");
        });
    }
    #[test]
    fn test_read_empty_root() {
        let xml_string = f_str!("<root attr=\"x\"/>");
        Python::initialize();
        Python::attach(|py| {
            let handle = read_string(py, xml_string, f_str!("root")).unwrap();
            let node = handle.borrow(py);
            assert_eq!(node.attrs.get("attr").unwrap(), "x");
            assert_eq!(node.children.len(), 0);
        });
    }
    #[test]
    fn test_read_mismatched_end_tag_raises_value_error() {
        let xml_string = f_str!("<root><a></b></root>");
        Python::initialize();
        Python::attach(|py| {
            let error = read_string(py, xml_string, f_str!("root")).unwrap_err();
            assert!(error.is_instance_of::<PyValueError>(py));
        });
    }
    #[test]
    fn test_read_unclosed_tag_raises_value_error() {
        let xml_string = f_str!("<root><a>");
        Python::initialize();
        Python::attach(|py| {
            let error = read_string(py, xml_string, f_str!("root")).unwrap_err();
            assert!(error.is_instance_of::<PyValueError>(py));
            assert!(error.to_string().contains("never closed"));
        });
    }
    #[test]
    fn test_read_extracts_nested_root_tag() {
        let xml_string = f_str!("<data><meta/><root a=\"1\">x</root></data>");
        Python::initialize();
        Python::attach(|py| {
            let handle = read_string(py, xml_string, f_str!("root")).unwrap();
            let node = handle.borrow(py);
            assert_eq!(node.attrs.get("a").unwrap(), "1");
            assert_eq!(node.text.as_ref().unwrap(), "x");
        });
    }
    #[test]
    fn test_read_missing_root_tag_raises_value_error() {
        let xml_string = f_str!("<other>content</other>");
        Python::initialize();
        Python::attach(|py| {
            let error = read_string(py, xml_string, f_str!("root")).unwrap_err();
            assert!(error.is_instance_of::<PyValueError>(py));
            assert!(error.to_string().contains("not found"));
        });
    }
    #[test]
    fn test_read_cdata_is_literal_text() {
        let xml_string = f_str!("<root>a <![CDATA[<b> &amp; ]]> c</root>");
        Python::initialize();
        Python::attach(|py| {
            let handle = read_string(py, xml_string, f_str!("root")).unwrap();
            assert_eq!(handle.borrow(py).text.as_ref().unwrap(), "a <b> &amp;  c");
        });
    }
    #[test]
    fn test_read_attr_syntax_and_normalization() {
        let xml_string = f_str!("<root a = 'x\ty' b=\"&#10;&#x41;\"/>");
        Python::initialize();
        Python::attach(|py| {
            let handle = read_string(py, xml_string, f_str!("root")).unwrap();
            let node = handle.borrow(py);
            assert_eq!(node.attrs.get("a").unwrap(), "x y");
            assert_eq!(node.attrs.get("b").unwrap(), "\nA");
        });
    }
    #[test]
    fn test_read_deeply_nested() {
        let depth = 100;
        let xml_string = format!("{}x{}", "<n>".repeat(depth), "</n>".repeat(depth));
        Python::initialize();
        Python::attach(|py| {
            let mut handle = read_string(py, xml_string, f_str!("n")).unwrap();
            for _ in 1..depth {
                let child = handle.borrow(py).children[0].clone_ref(py);
                handle = child;
            }
            let leaf = handle.borrow(py);
            assert!(leaf.children.is_empty());
            assert_eq!(leaf.text.as_ref().unwrap(), "x");
        });
    }
    #[test]
    fn test_read_malformed_documents_raise_value_error() {
        let cases = [
            "<root><!-- never closed</root>",
            "<root><![CDATA[never closed</root>",
            "<root a=\"1\" a=\"2\"/>",
            "<root a=1/>",
            "<root>&unknown;</root>",
            "<root>a & b</root>",
            "<root>a < b</root>",
        ];
        Python::initialize();
        Python::attach(|py| {
            for xml in cases {
                let error = read_string(py, f_str!(xml), f_str!("root"))
                    .err()
                    .unwrap_or_else(|| panic!("expected an error for {xml:?}"));
                assert!(error.is_instance_of::<PyValueError>(py), "{xml:?}: {error}");
            }
        });
    }
    #[test]
    fn test_read_missing_file_raises_file_not_found() {
        Python::initialize();
        Python::attach(|py| {
            let error =
                read_file(py, f_str!("tests/does_not_exist.xml"), f_str!("root")).unwrap_err();
            assert!(error.is_instance_of::<PyFileNotFoundError>(py));
        });
    }
}
