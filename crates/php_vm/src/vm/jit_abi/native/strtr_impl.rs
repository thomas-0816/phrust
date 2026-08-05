//! Total native `strtr($subject, $replacements)` publication.

use super::*;

impl NativeRequestFastState {
    fn native_strtr_map_render(
        &self,
        subject: &[u8],
        entries: &[php_jit::JitNativeDirectArrayEntry],
        mut output: Option<&mut [u8]>,
    ) -> Option<usize> {
        let mut input_offset = 0_usize;
        let mut output_offset = 0_usize;
        while input_offset < subject.len() {
            let remaining = &subject[input_offset..];
            let mut matched_key_length = 0_usize;
            let mut matched_value = None;
            for entry in entries {
                let key = self.native_string_view(entry.key)?;
                if key.is_empty() {
                    return None;
                }
                let value = self.native_string_view(entry.value)?;
                if key.len() > matched_key_length && remaining.starts_with(key) {
                    matched_key_length = key.len();
                    matched_value = Some(value);
                }
            }
            if let Some(value) = matched_value {
                let end = output_offset.checked_add(value.len())?;
                if let Some(output) = output.as_deref_mut() {
                    output.get_mut(output_offset..end)?.copy_from_slice(value);
                }
                output_offset = end;
                input_offset = input_offset.checked_add(matched_key_length)?;
            } else {
                let end = output_offset.checked_add(1)?;
                if let Some(output) = output.as_deref_mut() {
                    *output.get_mut(output_offset)? = subject[input_offset];
                }
                output_offset = end;
                input_offset = input_offset.checked_add(1)?;
            }
        }
        Some(output_offset)
    }

    #[allow(unsafe_code)] // Safety: all input owners and native arena bases remain stable during synchronous publication.
    pub(super) fn publish_direct_strtr_map(
        &mut self,
        subject: i64,
        replacements: i64,
    ) -> Option<i64> {
        let (subject, subject_length) = self.stable_native_string_range(subject)?;
        let entries = self.native_direct_array_entries(replacements)?;
        let entries_pointer = entries.as_ptr();
        let entries_length = entries.len();
        let output_length = self.native_strtr_map_render(
            unsafe { std::slice::from_raw_parts(subject, subject_length) },
            entries,
            None,
        )?;
        let state = std::ptr::from_ref(self);
        self.try_publish_direct_string_with(output_length, |output| {
            let rendered = unsafe {
                (&*state).native_strtr_map_render(
                    std::slice::from_raw_parts(subject, subject_length),
                    std::slice::from_raw_parts(entries_pointer, entries_length),
                    Some(output),
                )
            };
            (rendered == Some(output_length))
                .then_some(())
                .ok_or("native strtr map changed after its length pass")
        })
        .ok()
    }
}
