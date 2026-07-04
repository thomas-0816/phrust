//! Request-local literal interning for performance VM execution.

use std::collections::BTreeMap;

use php_runtime::PhpString;

/// Request-local literal pool.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LiteralPool {
    strings: BTreeMap<Vec<u8>, PhpString>,
    hits: u64,
    misses: u64,
}

impl LiteralPool {
    /// Interns raw PHP string bytes and returns a COW-safe string handle.
    ///
    /// Literals go through the thread-local symbol interner, so pooled
    /// strings carry a symbol identity for fast equality/hashing (array
    /// string keys from literals, names) in addition to sharing storage.
    pub fn intern_bytes(&mut self, bytes: &[u8]) -> InternedLiteral {
        if let Some(value) = self.strings.get(bytes) {
            self.hits += 1;
            return InternedLiteral {
                value: value.clone(),
                hit: true,
            };
        }
        let value = PhpString::intern(bytes);
        self.strings.insert(bytes.to_vec(), value.clone());
        self.misses += 1;
        InternedLiteral { value, hit: false }
    }

    /// Interns a UTF-8 Rust string as PHP bytes.
    pub fn intern_str(&mut self, value: &str) -> InternedLiteral {
        self.intern_bytes(value.as_bytes())
    }

    /// Number of cache hits since the pool was created/reset.
    #[must_use]
    pub const fn hits(&self) -> u64 {
        self.hits
    }

    /// Number of cache misses since the pool was created/reset.
    #[must_use]
    pub const fn misses(&self) -> u64 {
        self.misses
    }

    /// Number of unique interned string entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// True when no literals have been interned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

/// Result of one interning request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternedLiteral {
    /// Interned PHP string handle.
    pub value: PhpString,
    /// True when the pool already contained this byte sequence.
    pub hit: bool,
}

#[cfg(test)]
mod tests {
    use super::LiteralPool;

    #[test]
    fn identical_strings_share_storage_but_cow_remains_safe() {
        let mut pool = LiteralPool::default();
        let first = pool.intern_str("literal");
        let second = pool.intern_str("literal");

        assert!(!first.hit);
        assert!(second.hit);
        assert_eq!(pool.hits(), 1);
        assert_eq!(pool.misses(), 1);
        assert_eq!(pool.len(), 1);
        assert!(first.value.shares_storage_with(&second.value));

        let mut mutated = second.value.clone();
        mutated.bytes_mut()[0] = b'L';

        assert_eq!(first.value.as_bytes(), b"literal");
        assert_eq!(mutated.as_bytes(), b"Literal");
        assert!(!first.value.shares_storage_with(&mutated));
    }

    #[test]
    fn distinct_literals_are_misses() {
        let mut pool = LiteralPool::default();

        assert!(!pool.intern_str("a").hit);
        assert!(!pool.intern_str("b").hit);

        assert_eq!(pool.hits(), 0);
        assert_eq!(pool.misses(), 2);
        assert_eq!(pool.len(), 2);
    }
}
