use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// A cheap, `Copy` handle to a string interned in a [`StringsPool`].
///
/// Backed by an index rather than a byte range so resolving it is a single
/// `Vec` indirection, and existing handles stay valid even as the pool grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringRef(u32);

impl fmt::Display for StringRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StringRef({})", self.0)
    }
}

/// An append-only string interner.
///
/// Each interned string is allocated once behind an `Rc<str>`; `entries` and
/// `lookup` both hold clones of that same `Rc`, so the dedup cache never
/// duplicates the string's bytes. .
#[derive(Debug, Default)]
pub(crate) struct StringsPool {
    entries: Vec<Rc<str>>,
    lookup: HashMap<Rc<str>, StringRef>,
}

impl StringsPool {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            lookup: HashMap::new(),
        }
    }

    /// Interns `string`, returning a stable handle. Interning an equal string
    /// again returns the same `StringRef` (deduplicated via `lookup`).
    pub fn intern(&mut self, string: &str) -> StringRef {
        if let Some(r) = self.lookup.get(string) {
            return *r;
        }

        assert!(self.entries.len() <= u32::MAX as usize);
        let rc: Rc<str> = Rc::from(string);
        let r = StringRef(self.entries.len() as u32);
        self.entries.push(Rc::clone(&rc));
        self.lookup.insert(rc, r);
        r
    }

    pub fn get(&self, str_ref: StringRef) -> Option<&str> {
        self.entries.get(str_ref.0 as usize).map(|s| s.as_ref())
    }
}
