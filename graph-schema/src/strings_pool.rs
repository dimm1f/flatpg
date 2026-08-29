use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// A cheap, `Copy` handle to a string interned in the graph's string pool.
///
/// Backed by an index rather than a byte range so resolving it is a single
/// `Vec` indirection, and existing handles stay valid even as the pool grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RawStringId(u32);

impl fmt::Display for RawStringId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RawStringId({})", self.0)
    }
}

/// An append-only string interner.
///
/// Each interned string is allocated once behind an `Rc<str>`; `entries` and
/// `lookup` both hold clones of that same `Rc`, so the dedup cache never
/// duplicates the string's bytes. .
#[derive(Debug, Default)]
pub struct StringsPool {
    entries: Vec<Arc<str>>,
    lookup: HashMap<Arc<str>, RawStringId>,
}

impl StringsPool {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            lookup: HashMap::new(),
        }
    }

    /// Interns `string`, returning a stable handle. Interning an equal string
    /// again returns the same `RawStringId` (deduplicated via `lookup`).
    pub fn intern(&mut self, string: &str) -> RawStringId {
        if let Some(r) = self.lookup.get(string) {
            return *r;
        }

        assert!(self.entries.len() <= u32::MAX as usize);
        let rc: Arc<str> = Arc::from(string);
        let r = RawStringId(self.entries.len() as u32);
        self.entries.push(Arc::clone(&rc));
        self.lookup.insert(rc, r);
        r
    }

    pub fn get(&self, string_id: RawStringId) -> Option<&str> {
        self.entries.get(string_id.0 as usize).map(|s| s.as_ref())
    }

    pub fn get_arc(&self, string_id: RawStringId) -> Option<Arc<str>> {
        self.entries.get(string_id.0 as usize).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_string_id_display_shows_the_index() {
        let mut pool = StringsPool::new();
        let id = pool.intern("hello");
        assert_eq!(id.to_string(), "RawStringId(0)");
    }

    #[test]
    fn intern_deduplicates_equal_strings() {
        let mut pool = StringsPool::new();
        let a = pool.intern("hello");
        let b = pool.intern("hello");
        let c = pool.intern("world");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn get_resolves_an_interned_string_and_none_for_a_foreign_id() {
        let mut pool = StringsPool::new();
        let id = pool.intern("hello");
        assert_eq!(pool.get(id), Some("hello"));

        let other_pool = StringsPool::new();
        assert_eq!(other_pool.get(id), None);
    }

    #[test]
    fn get_arc_resolves_an_interned_string_and_none_for_a_foreign_id() {
        let mut pool = StringsPool::new();
        let id = pool.intern("hello");
        assert_eq!(pool.get_arc(id).as_deref(), Some("hello"));

        let other_pool = StringsPool::new();
        assert_eq!(other_pool.get_arc(id), None);
    }
}
