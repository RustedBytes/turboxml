//! Strings either own decoded data or refer to one immutable document buffer.
use pyo3::pybacked::PyBackedStr;
use std::borrow::{Borrow, Cow};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, Range};
use std::sync::Arc;

pub enum Source {
    Python(PyBackedStr),
    File(String),
}

impl Source {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Python(text) => text.as_str(),
            Self::File(text) => text,
        }
    }
}

#[derive(Clone)]
pub enum XmlText {
    Owned(String),
    View {
        source: Arc<Source>,
        range: Range<usize>,
    },
}

impl XmlText {
    /// `text` is a slice of this document. The owner remains alive in the view.
    pub fn view(source: &Arc<Source>, text: &str) -> Self {
        let start = text.as_ptr() as usize - source.as_str().as_ptr() as usize;
        let range = start..start + text.len();
        debug_assert_eq!(&source.as_str()[range.clone()], text);
        Self::View {
            source: Arc::clone(source),
            range,
        }
    }

    pub fn from_cow(source: &Arc<Source>, text: Cow<'_, str>) -> Self {
        match text {
            Cow::Borrowed(text) => Self::view(source, text),
            Cow::Owned(text) => Self::Owned(text),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Owned(text) => text,
            Self::View { source, range } => &source.as_str()[range.clone()],
        }
    }
}

impl From<String> for XmlText {
    fn from(text: String) -> Self {
        Self::Owned(text)
    }
}
impl Deref for XmlText {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}
impl AsRef<str> for XmlText {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl Borrow<str> for XmlText {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}
impl<T: AsRef<str> + ?Sized> PartialEq<T> for XmlText {
    fn eq(&self, other: &T) -> bool {
        self.as_str() == other.as_ref()
    }
}
impl Eq for XmlText {}
impl Hash for XmlText {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}
impl fmt::Display for XmlText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}
impl fmt::Debug for XmlText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}
