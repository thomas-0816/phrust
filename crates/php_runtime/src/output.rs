//! Byte-oriented output buffering.

use std::collections::BTreeMap;

use crate::string::PhpString;

/// Runtime output buffer.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OutputStats {
    /// Final/root-visible bytes are computed by the VM from `OutputBuffer::len`.
    pub appends: u64,
    /// Writes that appended more than one slice after one reserve.
    pub batch_writes: u64,
    /// Active output-buffer flushes into a parent or root buffer.
    pub flushes: u64,
    /// Appends that used a VM-proven exact-output fast path.
    pub fast_appends: u64,
    /// Generic conversion/output appends grouped by stable fallback reason.
    pub slow_appends_by_reason: BTreeMap<String, u64>,
}

/// Runtime output buffer.
#[derive(Clone, Debug, Default)]
pub struct OutputBuffer {
    bytes: Vec<u8>,
    stack: Vec<Vec<u8>>,
    stats: OutputStats,
}

impl PartialEq for OutputBuffer {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes && self.stack == other.stack
    }
}

impl Eq for OutputBuffer {}

impl OutputBuffer {
    /// Creates an empty output buffer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bytes: Vec::new(),
            stack: Vec::new(),
            stats: OutputStats::default(),
        }
    }

    /// Creates an empty output buffer with root buffer capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
            stack: Vec::new(),
            stats: OutputStats::default(),
        }
    }

    /// Returns request-local output write statistics.
    #[must_use]
    pub fn stats(&self) -> OutputStats {
        self.stats.clone()
    }

    /// Reserves capacity in the active buffer.
    pub fn reserve(&mut self, additional: usize) {
        if additional == 0 {
            return;
        }
        if let Some(buffer) = self.stack.last_mut() {
            buffer.reserve(additional);
        } else {
            self.bytes.reserve(additional);
        }
    }

    /// Appends exact bytes.
    pub fn write_bytes(&mut self, bytes: impl AsRef<[u8]>) {
        let bytes = bytes.as_ref();
        if bytes.is_empty() {
            return;
        }
        self.stats.appends += 1;
        if let Some(buffer) = self.stack.last_mut() {
            buffer.extend_from_slice(bytes);
        } else {
            self.bytes.extend_from_slice(bytes);
        }
    }

    /// Appends exact bytes through a VM-proven fast path.
    pub fn write_fast_bytes(&mut self, bytes: impl AsRef<[u8]>) {
        let bytes = bytes.as_ref();
        if bytes.is_empty() {
            return;
        }
        self.stats.fast_appends += 1;
        self.write_bytes(bytes);
    }

    /// Appends several byte slices with one active-buffer reservation.
    pub fn write_slices(&mut self, slices: &[&[u8]]) {
        let total = slices.iter().map(|bytes| bytes.len()).sum::<usize>();
        if total == 0 {
            return;
        }
        self.stats.appends += 1;
        if slices
            .iter()
            .filter(|bytes| !bytes.is_empty())
            .take(2)
            .count()
            > 1
        {
            self.stats.batch_writes += 1;
        }
        if let Some(buffer) = self.stack.last_mut() {
            buffer.reserve(total);
            for bytes in slices.iter().copied().filter(|bytes| !bytes.is_empty()) {
                buffer.extend_from_slice(bytes);
            }
        } else {
            self.bytes.reserve(total);
            for bytes in slices.iter().copied().filter(|bytes| !bytes.is_empty()) {
                self.bytes.extend_from_slice(bytes);
            }
        }
    }

    /// Appends several byte slices through a VM-proven fast path.
    pub fn write_fast_slices(&mut self, slices: &[&[u8]]) {
        let has_bytes = slices.iter().any(|bytes| !bytes.is_empty());
        if !has_bytes {
            return;
        }
        self.stats.fast_appends += 1;
        self.write_slices(slices);
    }

    /// Appends a PHP string's exact bytes.
    pub fn write_php_string(&mut self, value: &PhpString) {
        self.write_bytes(value.as_bytes());
    }

    /// Appends a PHP string's exact bytes through a VM-proven fast path.
    pub fn write_fast_php_string(&mut self, value: &PhpString) {
        self.write_fast_bytes(value.as_bytes());
    }

    /// Records that output had to use a generic conversion/fallback path.
    pub fn record_slow_append_reason(&mut self, reason: &'static str) {
        *self
            .stats
            .slow_appends_by_reason
            .entry(reason.to_string())
            .or_default() += 1;
    }

    /// Convenience for tests and ASCII literals.
    pub fn write_test_str(&mut self, text: &str) {
        self.write_bytes(text.as_bytes());
    }

    /// Returns the exact output bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the exact buffered byte count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns true when no output bytes have been buffered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Consumes the buffer and returns exact output bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// Test/debug convenience for textual assertions.
    #[must_use]
    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    /// Clears buffered output.
    pub fn clear(&mut self) {
        self.bytes.clear();
        self.stack.clear();
    }

    /// Starts a nested PHP output buffer.
    pub fn start_buffer(&mut self) {
        self.stack.push(Vec::new());
    }

    /// Returns the current output buffering level.
    #[must_use]
    pub fn buffer_level(&self) -> usize {
        self.stack.len()
    }

    /// Returns the active buffer contents, if output buffering is active.
    #[must_use]
    pub fn current_buffer_bytes(&self) -> Option<&[u8]> {
        self.stack.last().map(Vec::as_slice)
    }

    /// Returns the active buffer length, if output buffering is active.
    #[must_use]
    pub fn current_buffer_len(&self) -> Option<usize> {
        self.stack.last().map(Vec::len)
    }

    /// Discards and returns the active buffer.
    pub fn pop_buffer_clean(&mut self) -> Option<Vec<u8>> {
        self.stack.pop()
    }

    /// Flushes the active buffer into its parent buffer or root output.
    pub fn pop_buffer_flush(&mut self) -> Option<()> {
        let bytes = self.stack.pop()?;
        self.stats.flushes += 1;
        self.write_bytes(bytes);
        Some(())
    }

    /// Flushes all open buffers to root output in shutdown order.
    pub fn flush_all_buffers(&mut self) {
        while self.pop_buffer_flush().is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::OutputBuffer;

    #[test]
    fn nested_buffers_capture_clean_and_flush() {
        let mut output = OutputBuffer::new();
        output.write_test_str("root");
        output.start_buffer();
        output.write_test_str("a");
        output.start_buffer();
        output.write_test_str("b");

        assert_eq!(output.as_bytes(), b"root");
        assert_eq!(output.buffer_level(), 2);
        assert_eq!(output.current_buffer_bytes(), Some(&b"b"[..]));
        assert_eq!(output.pop_buffer_clean(), Some(b"b".to_vec()));
        assert_eq!(output.current_buffer_bytes(), Some(&b"a"[..]));
        assert_eq!(output.pop_buffer_flush(), Some(()));
        assert_eq!(output.as_bytes(), b"roota");
        assert_eq!(output.stats().appends, 4);
        assert_eq!(output.stats().batch_writes, 0);
        assert_eq!(output.stats().flushes, 1);
        assert_eq!(output.stats().fast_appends, 0);
        assert!(output.stats().slow_appends_by_reason.is_empty());
    }

    #[test]
    fn batch_write_reserves_and_counts_one_append() {
        let mut output = OutputBuffer::new();

        output.write_slices(&[b"a", b"", b"bc"]);

        assert_eq!(output.as_bytes(), b"abc");
        assert_eq!(output.stats().appends, 1);
        assert_eq!(output.stats().batch_writes, 1);
        assert_eq!(output.stats().flushes, 0);
    }

    #[test]
    fn fast_and_slow_output_stats_are_stable() {
        let mut output = OutputBuffer::new();

        output.write_fast_bytes(b"a");
        output.write_fast_slices(&[b"b", b"c"]);
        output.record_slow_append_reason("object_to_string");
        output.record_slow_append_reason("object_to_string");

        assert_eq!(output.as_bytes(), b"abc");
        assert_eq!(output.stats().appends, 2);
        assert_eq!(output.stats().batch_writes, 1);
        assert_eq!(output.stats().fast_appends, 2);
        assert_eq!(
            output
                .stats()
                .slow_appends_by_reason
                .get("object_to_string"),
            Some(&2)
        );
    }
}
