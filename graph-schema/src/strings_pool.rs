use std::fmt;

/// Multiplier from rustc's `FxHasher`.
const HASH_MULTIPLIER: u64 = 0x51_7c_c1_b7_27_22_0a_95;

/// Lookup table size for the first interned string.
const MIN_LOOKUP_SLOTS: usize = 16;

#[inline]
fn mix(hash: u64, word: u64) -> u64 {
    (hash.rotate_left(5) ^ word).wrapping_mul(HASH_MULTIPLIER)
}

/// Hashes `string` with rustc's `FxHash` construction, folded for low-bit indexing.
///
/// The pool hashes only its own strings and never exposes the result, so it trades HashDoS
/// resistance for a few instructions per word. The tail is zero-padded, so the length is mixed
/// in to keep strings that differ only in trailing NUL bytes apart. The final fold is
/// load-bearing: multiplication carries entropy upwards only, and the weak low bits it leaves
/// are exactly the ones [`StringsPool::find`] indexes by.
fn hash_str(string: &str) -> u32 {
    let bytes = string.as_bytes();
    let mut hash = 0u64;

    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        let mut word = [0u8; 8];
        word.copy_from_slice(chunk);
        hash = mix(hash, u64::from_le_bytes(word));
    }

    let remainder = chunks.remainder();
    let mut tail = [0u8; 8];
    tail[..remainder.len()].copy_from_slice(remainder);
    let hash = mix(hash, u64::from_le_bytes(tail) ^ bytes.len() as u64);

    (hash ^ (hash >> 32)) as u32
}

/// Derives a slot's occupancy tag from a string's hash.
///
/// Taken from bits the table's mask never indexes by, so the tag filters independently of the
/// slot a string lands in. Forcing the low bit on keeps zero free to mark an empty slot, which
/// costs one bit of filtering power and saves an occupancy bitmap.
#[inline]
fn tag_of(hash: u32) -> u8 {
    (hash >> 24) as u8 | 1
}

fn insert_slot(tags: &mut [u8], slots: &mut [u32], hash: u32, id: u32) {
    let mask = tags.len() - 1;
    let mut slot = hash as usize & mask;
    while tags[slot] != 0 {
        slot = (slot + 1) & mask;
    }
    tags[slot] = tag_of(hash);
    slots[slot] = id;
}

/// A cheap, `Copy` handle to a string interned in a [`StringsPool`].
///
/// A handle is meaningful only for the pool that interned it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RawStringId(u32);

impl RawStringId {
    pub(crate) fn index(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for RawStringId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RawStringId({})", self.0)
    }
}

/// An append-only string interner.
///
/// Interning an equal string again returns the same [`RawStringId`]. New strings never
/// invalidate a handle the pool has already returned. Resolving a handle costs a bounds check
/// and two indexed loads. Interning costs one hash of the string, plus one comparison for each
/// probe whose hash tag matches.
///
/// A handle only means something to the pool that made it. [`StringsPool::get`] cannot tell a
/// foreign handle from its own. It reads whatever string sits at that index, and returns `None`
/// only when the index is past the end.
///
/// The public API never removes a string, so a pool only grows. It holds up to `u32::MAX`
/// strings and up to `u32::MAX` bytes of text.
#[derive(Debug, Default)]
pub struct StringsPool {
    buffer: String,
    ranges: Vec<(u32, u32)>,
    hashes: Vec<u32>,
    tags: Vec<u8>,
    slots: Vec<u32>,
}

impl StringsPool {
    /// Creates an empty pool.
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            ranges: Vec::new(),
            hashes: Vec::new(),
            tags: Vec::new(),
            slots: Vec::new(),
        }
    }

    /// Interns `string`, returning a stable handle.
    ///
    /// Interning an equal string again returns the same [`RawStringId`].
    ///
    /// # Panics
    ///
    /// Panics if the pool would exceed `u32::MAX` interned strings or `u32::MAX` bytes of text.
    pub fn intern(&mut self, string: &str) -> RawStringId {
        let hash = hash_str(string);
        if let Some(id) = self.find(string, hash) {
            return id;
        }

        assert!(
            self.ranges.len() < u32::MAX as usize,
            "strings pool holds at most u32::MAX strings"
        );
        let start = self.buffer.len();
        assert!(
            start + string.len() <= u32::MAX as usize,
            "strings pool holds at most u32::MAX bytes of text"
        );

        let id = self.ranges.len() as u32;
        self.buffer.push_str(string);
        self.ranges.push((start as u32, string.len() as u32));
        self.hashes.push(hash);

        // Kept below half full: a miss probes to the first free slot, ~2.5 probes at one half
        // against ~8.5 at three quarters, and that free slot is what ends the probe.
        if self.ranges.len() * 2 >= self.tags.len() {
            self.rebuild_lookup((self.tags.len() * 2).max(MIN_LOOKUP_SLOTS));
        } else {
            insert_slot(&mut self.tags, &mut self.slots, hash, id);
        }

        RawStringId(id)
    }

    /// Resolves a handle to its string, or `None` if its index is past the end of this pool.
    #[inline]
    pub fn get(&self, string_id: RawStringId) -> Option<&str> {
        self.ranges
            .get(string_id.0 as usize)
            .map(|&range| self.slice(range))
    }

    /// Returns how many strings are interned.
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Returns `true` when nothing is interned yet.
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Drops every string interned at or after `len`, invalidating their handles.
    pub(crate) fn truncate(&mut self, len: usize) {
        if len >= self.ranges.len() {
            return;
        }
        let end = self.ranges[len].0 as usize;
        self.ranges.truncate(len);
        self.hashes.truncate(len);
        self.buffer.truncate(end);
        self.rebuild_lookup(self.tags.len());
    }

    #[inline]
    fn resolve(&self, index: usize) -> &str {
        self.slice(self.ranges[index])
    }

    #[inline]
    fn slice(&self, (start, len): (u32, u32)) -> &str {
        let (start, len) = (start as usize, len as usize);

        // Safety: `start` and `len` come from a `ranges` entry, and `intern` pushes each entry
        // as the start and length of one whole `&str` it appended to `buffer`
        unsafe { self.buffer.get_unchecked(start..start + len) }
    }

    fn find(&self, string: &str, hash: u32) -> Option<RawStringId> {
        if self.tags.is_empty() {
            return None;
        }

        let mask = self.tags.len() - 1;
        let tag = tag_of(hash);
        let mut slot = hash as usize & mask;
        loop {
            let slot_tag = self.tags[slot];
            if slot_tag == 0 {
                return None;
            }
            if slot_tag == tag {
                let id = self.slots[slot];
                if self.resolve(id as usize) == string {
                    return Some(RawStringId(id));
                }
            }
            slot = (slot + 1) & mask;
        }
    }

    fn rebuild_lookup(&mut self, capacity: usize) {
        let capacity = capacity.max(MIN_LOOKUP_SLOTS);
        let mut tags = vec![0u8; capacity];
        let mut slots = vec![0u32; capacity];
        for (index, &hash) in self.hashes.iter().enumerate() {
            insert_slot(&mut tags, &mut slots, hash, index as u32);
        }
        self.tags = tags;
        self.slots = slots;
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

    /// Locks the rollback `GraphDiff::prepare` and `StagedDiff`'s `Drop` rely on: a truncated
    /// string must leave the dedup cache too, or re-interning it would hand back a handle that
    /// no longer resolves.
    #[test]
    fn truncate_removes_entries_along_with_their_dedup_cache_slots() {
        let mut pool = StringsPool::new();
        pool.intern("kept");
        let dropped = pool.intern("dropped");

        pool.truncate(1);
        let reinterned = pool.intern("dropped");

        assert_eq!(pool.len(), 2);
        assert_eq!(reinterned, dropped);
        assert_eq!(pool.get(reinterned), Some("dropped"));
    }

    #[test]
    fn truncate_past_the_last_entry_keeps_the_pool_as_it_is() {
        let mut pool = StringsPool::new();
        let id = pool.intern("hello");

        pool.truncate(5);

        assert_eq!(pool.len(), 1);
        assert_eq!(pool.get(id), Some("hello"));
    }

    #[test]
    fn truncate_to_zero_empties_the_pool() {
        let mut pool = StringsPool::new();
        pool.intern("a");
        pool.intern("b");

        pool.truncate(0);

        assert!(pool.is_empty());
        assert_eq!(pool.intern("b").to_string(), "RawStringId(0)");
        assert_eq!(pool.get(RawStringId(0)), Some("b"));
    }

    /// Drives the lookup table through several doublings, where a re-slot that dropped or
    /// duplicated an entry would surface as a wrong handle.
    #[test]
    fn many_strings_keep_distinct_handles_across_table_growth() {
        let mut pool = StringsPool::new();
        let values: Vec<String> = (0..1_000).map(|i| format!("value-{i}")).collect();

        let ids: Vec<RawStringId> = values.iter().map(|v| pool.intern(v)).collect();

        assert_eq!(pool.len(), values.len());
        for (value, id) in values.iter().zip(&ids) {
            assert_eq!(pool.get(*id), Some(value.as_str()));
            assert_eq!(
                pool.intern(value),
                *id,
                "re-interning must dedup to {value}"
            );
        }
    }

    #[test]
    fn strings_sharing_a_prefix_are_interned_separately() {
        let mut pool = StringsPool::new();
        let short = pool.intern("ab");
        let long = pool.intern("abc");

        assert_ne!(short, long);
        assert_eq!(pool.get(short), Some("ab"));
        assert_eq!(pool.get(long), Some("abc"));
    }

    #[test]
    fn multibyte_strings_resolve_whole() {
        let mut pool = StringsPool::new();
        let two_byte = pool.intern("caf\u{e9}");
        let three_byte = pool.intern("\u{20ac}");

        assert_eq!(pool.get(two_byte), Some("caf\u{e9}"));
        assert_eq!(pool.get(three_byte), Some("\u{20ac}"));
    }

    /// Locks the invariant `slice`'s `get_unchecked` rests on: `truncate` cuts the buffer at a
    /// stored range start, which is a character boundary, so what stays behind still resolves
    /// and what is interned next lands on a boundary too.
    #[test]
    fn truncating_multibyte_strings_leaves_the_buffer_on_a_boundary() {
        let mut pool = StringsPool::new();
        let kept = pool.intern("caf\u{e9}");
        pool.intern("\u{20ac}\u{20ac}");

        pool.truncate(1);
        let reinterned = pool.intern("na\u{ef}ve");

        assert_eq!(pool.get(kept), Some("caf\u{e9}"));
        assert_eq!(pool.get(reinterned), Some("na\u{ef}ve"));
        assert_eq!(pool.len(), 2);
    }
}
