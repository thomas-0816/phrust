#[allow(unsafe_code)]
fn native_callable_view_bytes_equal(
    left: u64,
    left_length: u32,
    right: u64,
    right_length: u32,
) -> bool {
    if left_length != right_length {
        return false;
    }
    if left_length == 0 {
        return true;
    }
    let length = left_length as usize;
    if left == 0 || right == 0 {
        return false;
    }
    unsafe {
        std::slice::from_raw_parts(left as usize as *const u8, length)
            == std::slice::from_raw_parts(right as usize as *const u8, length)
    }
}

#[derive(Clone, Copy)]
enum NativeHttpQueryPathSegment {
    Integer(i64),
    String(*const u8, usize),
}

const NATIVE_HTTP_QUERY_TRAVERSAL_CAPACITY: usize = 64;

struct NativeHttpQueryTraversal {
    active_arrays: [usize; NATIVE_HTTP_QUERY_TRAVERSAL_CAPACITY],
    active_array_count: usize,
    path: [NativeHttpQueryPathSegment; NATIVE_HTTP_QUERY_TRAVERSAL_CAPACITY],
    path_length: usize,
}

impl NativeHttpQueryTraversal {
    const fn new() -> Self {
        Self {
            active_arrays: [0; NATIVE_HTTP_QUERY_TRAVERSAL_CAPACITY],
            active_array_count: 0,
            path: [NativeHttpQueryPathSegment::Integer(0); NATIVE_HTTP_QUERY_TRAVERSAL_CAPACITY],
            path_length: 0,
        }
    }

    fn array_is_active(&self, index: usize) -> bool {
        self.active_arrays[..self.active_array_count].contains(&index)
    }

    fn push_array(&mut self, index: usize) -> Option<()> {
        *self.active_arrays.get_mut(self.active_array_count)? = index;
        self.active_array_count += 1;
        Some(())
    }

    fn pop_array(&mut self) {
        self.active_array_count = self.active_array_count.saturating_sub(1);
    }

    fn push_path(&mut self, segment: NativeHttpQueryPathSegment) -> Option<()> {
        *self.path.get_mut(self.path_length)? = segment;
        self.path_length += 1;
        Some(())
    }

    fn pop_path(&mut self) {
        self.path_length = self.path_length.saturating_sub(1);
    }

    fn path(&self) -> &[NativeHttpQueryPathSegment] {
        &self.path[..self.path_length]
    }
}

// architecture: fixed stack storage avoids a heap allocation in scalar conversion
#[allow(clippy::large_enum_variant)]
enum NativeScalarBytes<'a> {
    Empty,
    Static(&'static [u8]),
    Borrowed(&'a [u8]),
    Integer {
        bytes: [u8; 20],
        start: usize,
    },
    Float {
        bytes: [u8; php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY],
        length: usize,
    },
}

impl NativeScalarBytes<'_> {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Empty => b"",
            Self::Static(bytes) => bytes,
            Self::Borrowed(bytes) => bytes,
            Self::Integer { bytes, start } => &bytes[*start..],
            Self::Float { bytes, length } => &bytes[..*length],
        }
    }

    fn into_lossy_owned(self) -> String {
        String::from_utf8_lossy(self.as_bytes()).into_owned()
    }
}

enum NativeDirectStringPublishError<E> {
    Arena(&'static str),
    Fill(E),
}

struct NativeDirectByteWriter<'a> {
    output: Option<&'a mut [u8]>,
    length: usize,
}

impl<'a> NativeDirectByteWriter<'a> {
    fn counting() -> Self {
        Self {
            output: None,
            length: 0,
        }
    }

    fn writing(output: &'a mut [u8]) -> Self {
        Self {
            output: Some(output),
            length: 0,
        }
    }

    fn write(&mut self, bytes: &[u8]) -> Option<()> {
        let end = self.length.checked_add(bytes.len())?;
        if let Some(output) = self.output.as_deref_mut() {
            output.get_mut(self.length..end)?.copy_from_slice(bytes);
        }
        self.length = end;
        Some(())
    }

    fn write_byte(&mut self, byte: u8) -> Option<()> {
        self.write(std::slice::from_ref(&byte))
    }

    fn write_i64(&mut self, value: i64) -> Option<()> {
        let mut buffer = [0_u8; 20];
        self.write(native_i64_ascii(value, &mut buffer))
    }

    fn write_usize(&mut self, value: usize) -> Option<()> {
        let mut buffer = [0_u8; 20];
        let mut magnitude = u64::try_from(value).ok()?;
        let mut cursor = buffer.len();
        loop {
            cursor -= 1;
            buffer[cursor] = b'0' + (magnitude % 10) as u8;
            magnitude /= 10;
            if magnitude == 0 {
                break;
            }
        }
        self.write(&buffer[cursor..])
    }

    fn is_complete(&self) -> bool {
        self.output
            .as_ref()
            .is_none_or(|output| self.length == output.len())
    }
}

struct NativeSerializationTraversal {
    active_arrays: [usize; NativeSerializedParser::MAX_DEPTH + 1],
    active_array_count: usize,
    active_references: [usize; NativeSerializedParser::MAX_DEPTH + 1],
    active_reference_count: usize,
}

const NATIVE_JSON_TRAVERSAL_CAPACITY: usize = 512;

struct NativeJsonTraversal {
    active_arrays: [usize; NATIVE_JSON_TRAVERSAL_CAPACITY],
    active_array_count: usize,
    active_objects: [u64; NATIVE_JSON_TRAVERSAL_CAPACITY],
    active_object_count: usize,
}

impl NativeJsonTraversal {
    fn new() -> Self {
        Self {
            active_arrays: [0; NATIVE_JSON_TRAVERSAL_CAPACITY],
            active_array_count: 0,
            active_objects: [0; NATIVE_JSON_TRAVERSAL_CAPACITY],
            active_object_count: 0,
        }
    }

    fn array_is_active(&self, index: usize) -> bool {
        self.active_arrays[..self.active_array_count].contains(&index)
    }

    fn push_array(&mut self, index: usize) -> Option<()> {
        *self.active_arrays.get_mut(self.active_array_count)? = index;
        self.active_array_count += 1;
        Some(())
    }

    fn pop_array(&mut self) {
        self.active_array_count = self.active_array_count.saturating_sub(1);
    }

    fn object_is_active(&self, identity: u64) -> bool {
        self.active_objects[..self.active_object_count].contains(&identity)
    }

    fn push_object(&mut self, identity: u64) -> Option<()> {
        *self.active_objects.get_mut(self.active_object_count)? = identity;
        self.active_object_count += 1;
        Some(())
    }

    fn pop_object(&mut self) {
        self.active_object_count = self.active_object_count.saturating_sub(1);
    }
}

impl NativeSerializationTraversal {
    fn new() -> Self {
        Self {
            active_arrays: [0; NativeSerializedParser::MAX_DEPTH + 1],
            active_array_count: 0,
            active_references: [0; NativeSerializedParser::MAX_DEPTH + 1],
            active_reference_count: 0,
        }
    }

    fn array_is_active(&self, index: usize) -> bool {
        self.active_arrays[..self.active_array_count].contains(&index)
    }

    fn push_array(&mut self, index: usize) -> Option<()> {
        *self.active_arrays.get_mut(self.active_array_count)? = index;
        self.active_array_count += 1;
        Some(())
    }

    fn pop_array(&mut self) {
        self.active_array_count = self.active_array_count.saturating_sub(1);
    }

    fn reference_is_active(&self, index: usize) -> bool {
        self.active_references[..self.active_reference_count].contains(&index)
    }

    fn push_reference(&mut self, index: usize) -> Option<()> {
        *self
            .active_references
            .get_mut(self.active_reference_count)? = index;
        self.active_reference_count += 1;
        Some(())
    }

    fn pop_reference(&mut self) {
        self.active_reference_count = self.active_reference_count.saturating_sub(1);
    }
}

fn native_i64_ascii(value: i64, buffer: &mut [u8; 20]) -> &[u8] {
    let negative = value < 0;
    let mut magnitude = value.unsigned_abs();
    let mut cursor = buffer.len();
    loop {
        cursor -= 1;
        buffer[cursor] = b'0' + (magnitude % 10) as u8;
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    if negative {
        cursor -= 1;
        buffer[cursor] = b'-';
    }
    &buffer[cursor..]
}

impl NativeRequestFastState {
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn active_call_arguments(&self) -> Option<&[i64]> {
        let view = self.header.active_runtime_view();
        if view.active_call_tail_arguments != 0 {
            return None;
        }
        let length = usize::try_from(view.active_call_argument_count).ok()?;
        if length == 0 {
            return Some(&[]);
        }
        let arguments = view.active_call_arguments as usize as *const i64;
        (!arguments.is_null()).then(|| unsafe { std::slice::from_raw_parts(arguments, length) })
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    pub(super) fn active_call_argument(&self, index: usize) -> Option<i64> {
        let view = self.header.active_runtime_view();
        if index >= usize::try_from(view.active_call_argument_count).ok()? {
            return None;
        }
        let fixed_count = usize::try_from(view.active_call_fixed_argument_count).ok()?;
        if index < fixed_count {
            let fixed = if view.active_call_fixed_arguments != 0 {
                view.active_call_fixed_arguments
            } else {
                view.active_call_arguments
            };
            if fixed != 0 {
                return Some(unsafe { *(fixed as usize as *const i64).add(index) });
            }
        }
        if view.active_call_tail_arguments != 0 {
            return self
                .native_direct_array_entries(view.active_call_tail_arguments)?
                .get(index.checked_sub(fixed_count)?)
                .map(|entry| entry.value);
        }
        self.active_call_arguments()?.get(index).copied()
    }

    fn native_func_num_args(&mut self) -> Result<i64, &'static str> {
        let count = usize::try_from(self.header.active_runtime_view().active_call_argument_count)
            .map_err(|_| "active native call frame is unavailable")?;
        self.publish_direct_int(i64::try_from(count).unwrap_or(i64::MAX))
    }

    fn native_func_get_arg(&mut self, index: usize) -> Result<Option<i64>, &'static str> {
        let value = self.active_call_argument(index);
        let Some(value) = value else {
            return Ok(None);
        };
        self.retain_direct_encoded(value)?;
        Ok(Some(value))
    }

    fn native_func_get_args(&mut self) -> Result<i64, &'static str> {
        let view = self.header.active_runtime_view();
        let count = usize::try_from(view.active_call_argument_count)
            .map_err(|_| "active native call frame is unavailable")?;
        let fixed_count = usize::try_from(view.active_call_fixed_argument_count)
            .map_err(|_| "active native fixed-argument count is invalid")?
            .min(count);
        let fixed = if view.active_call_fixed_arguments != 0 {
            view.active_call_fixed_arguments
        } else {
            view.active_call_arguments
        } as usize as *const i64;
        let arguments = view.active_call_arguments as usize as *const i64;
        let (tail, tail_count) = if view.active_call_tail_arguments != 0 {
            let entries = self
                .native_direct_array_entries(view.active_call_tail_arguments)
                .ok_or("active native call tail is unavailable")?;
            (entries.as_ptr(), entries.len())
        } else {
            (std::ptr::null(), 0)
        };
        if (fixed_count != 0 && fixed.is_null())
            || (view.active_call_tail_arguments == 0 && count != 0 && arguments.is_null())
            || (view.active_call_tail_arguments != 0
                && tail_count < count.saturating_sub(fixed_count))
        {
            return Err("active native call argument storage is unavailable");
        }
        self.publish_retained_direct_array_from_iter((0..count).map(|index| {
            // SAFETY: generated call lowering keeps the fixed stack range and
            // direct tail array live until this synchronous exact call
            // returns. The target array arena is stable and cannot relocate
            // either source range.
            #[allow(unsafe_code)]
            let value = unsafe {
                if index < fixed_count {
                    *fixed.add(index)
                } else if !tail.is_null() {
                    (*tail.add(index - fixed_count)).value
                } else {
                    *arguments.add(index)
                }
            };
            php_jit::JitNativeDirectArrayEntry {
                key: i64::try_from(index).unwrap_or(i64::MAX),
                value,
            }
        }))
    }

    /// Borrows the two capabilities required by exact path/filesystem
    /// handlers. Their addresses are request-stable and no cold execution
    /// coordinator is recovered on this path.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_filesystem_capability(
        &self,
    ) -> Option<(&std::path::Path, &php_runtime::api::FilesystemCapabilities)> {
        let cwd = unsafe { self.cwd.as_ref() }?;
        let filesystem = unsafe { self.filesystem_capabilities.as_ref() }?;
        Some((cwd.as_path(), filesystem))
    }

    fn native_upload_registry(&self) -> Option<&php_runtime::api::UploadRegistry> {
        // SAFETY: the cold request owns the registry at a stable address for
        // the complete synchronous native activation.
        #[allow(unsafe_code)]
        unsafe {
            self.upload_registry.as_ref()
        }
    }

    /// Borrows the complete stable capability required by the exact uploaded-
    /// file move. The three pointers address disjoint request-owned boxes and
    /// remain stable for the synchronous native activation.
    fn native_upload_move_capability(
        &mut self,
    ) -> Option<(
        &std::path::Path,
        &php_runtime::api::FilesystemCapabilities,
        &mut php_runtime::api::UploadRegistry,
    )> {
        // SAFETY: publication initializes all three pointers from distinct
        // request-owned allocations before generated code can enter.
        #[allow(unsafe_code)]
        unsafe {
            Some((
                self.cwd.as_ref()?.as_path(),
                self.filesystem_capabilities.as_ref()?,
                self.upload_registry.as_mut()?,
            ))
        }
    }

    fn fill_random(&self, bytes: &mut [u8]) -> Option<()> {
        (self.random.fill?)(bytes).then_some(())
    }

    fn random_u128(&self) -> Option<u128> {
        let mut bytes = [0_u8; 16];
        self.fill_random(&mut bytes)?;
        Some(u128::from_le_bytes(bytes))
    }

    fn random_bounded_usize(&self, bound: usize) -> Option<usize> {
        let range = u128::try_from(bound).ok()?;
        if range == 0 {
            return None;
        }
        let zone = u128::MAX - (u128::MAX % range);
        loop {
            let sample = self.random_u128()?;
            if sample < zone {
                return usize::try_from(sample % range).ok();
            }
        }
    }

    fn random_int_inclusive(&self, minimum: i64, maximum: i64) -> Option<i64> {
        if maximum < minimum {
            return None;
        }
        let range = (i128::from(maximum) - i128::from(minimum) + 1) as u128;
        let zone = u128::MAX - (u128::MAX % range);
        loop {
            let sample = self.random_u128()?;
            if sample < zone {
                let offset = i128::try_from(sample % range).ok()?;
                return i64::try_from(i128::from(minimum) + offset).ok();
            }
        }
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_shuffle(&mut self, reference: i64) -> Option<()> {
        let (_, reference_slot) = self.direct_slot(reference)?;
        if reference_slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            || reference_slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
        {
            return None;
        }
        let array = reference_slot.payload as i64;
        let (array_index, array_slot) = self.direct_slot(array)?;
        if array_slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
            || array_slot.refcount != 1
        {
            return None;
        }
        let length = usize::try_from(array_slot.payload).ok()?;
        let entries = array_slot.aux as usize as *mut php_jit::JitNativeDirectArrayEntry;
        if length != 0 && entries.is_null() {
            return None;
        }
        // Resolve every random draw before mutating the authoritative array.
        // A failed capability can therefore take the one baseline
        // continuation with the original array untouched.
        let offsets = (0..length)
            .map(|index| self.random_bounded_usize(length - index))
            .collect::<Option<Vec<_>>>()?;
        for (index, offset) in offsets.into_iter().enumerate() {
            unsafe {
                std::ptr::swap(entries.add(index), entries.add(index + offset));
            }
        }
        for index in 0..length {
            let key = unsafe { (*entries.add(index)).key };
            let _ = self.discard_owned_direct_value(key);
            unsafe {
                (*entries.add(index)).key = i64::try_from(index).ok()?;
            }
        }
        let slots = self.header.active_runtime_view().direct_value_slots as usize
            as *mut php_jit::JitNativeValueSlot;
        unsafe {
            (*slots.add(array_index)).flags =
                php_jit::jit_native_direct_array_flags((length != 0).then_some(0));
        }
        Some(())
    }

    /// Borrows the request's current directory without granting filesystem
    /// access. `chdir()` updates this stable owner in place.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_current_directory(&self) -> Option<&std::path::Path> {
        unsafe { self.cwd.as_ref() }.map(std::path::PathBuf::as_path)
    }

    /// Borrows the authoritative mutable current-directory slot together with
    /// its immutable admission policy. Exact `chdir()` validates a target
    /// completely before replacing the slot.
    #[allow(unsafe_code)] // Safety: both pointers are request-owned and published for this activation.
    fn native_chdir_capability(
        &mut self,
    ) -> Option<(
        &mut std::path::PathBuf,
        &php_runtime::api::FilesystemCapabilities,
    )> {
        let cwd = unsafe { self.cwd.as_mut() }?;
        let filesystem = unsafe { self.filesystem_capabilities.as_ref() }?;
        Some((cwd, filesystem))
    }

    /// Borrows only the authoritative request-local filesystem process state.
    #[allow(unsafe_code)] // Safety: the request owner keeps the published slot stable.
    fn native_filesystem_state(&mut self) -> Option<&mut php_runtime::api::FilesystemRuntimeState> {
        unsafe { self.filesystem_state.as_mut() }
    }

    /// Borrows only the capabilities needed to open a stream. These pointers
    /// are published directly from request-owned fields; no cold execution
    /// coordinator or generic builtin registry is recovered.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_stream_open_capability(
        &mut self,
    ) -> Option<(
        &mut php_runtime::api::ResourceTable,
        &std::path::Path,
        &php_runtime::api::FilesystemCapabilities,
        &[u8],
    )> {
        let resources = unsafe { self.resources.as_mut() }?;
        let cwd = unsafe { self.cwd.as_ref() }?;
        let filesystem = unsafe { self.filesystem_capabilities.as_ref() }?;
        let stdin = unsafe { self.stdin.as_ref() }?;
        Some((resources, cwd.as_path(), filesystem, stdin.as_ref()))
    }

    /// Borrows only the resource registry needed to allocate stream-context
    /// capabilities. Context options remain in the native request arena.
    #[allow(unsafe_code)] // Safety: the request owns the published table for this activation.
    fn native_stream_context_resources(&mut self) -> Option<&mut php_runtime::api::ResourceTable> {
        unsafe { self.resources.as_mut() }
    }

    /// Borrows the narrow capability set used by exact directory handlers.
    #[allow(unsafe_code)] // Safety: the request owns every published pointer for the synchronous activation.
    fn native_directory_capability(
        &mut self,
    ) -> Option<(
        &mut php_runtime::api::ResourceTable,
        &std::path::Path,
        &php_runtime::api::FilesystemCapabilities,
    )> {
        let resources = unsafe { self.resources.as_mut() }?;
        let cwd = unsafe { self.cwd.as_ref() }?;
        let filesystem = unsafe { self.filesystem_capabilities.as_ref() }?;
        Some((resources, cwd.as_path(), filesystem))
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn reserve_direct_value_index(&mut self) -> Result<u32, &'static str> {
        let view = self.header.active_runtime_view();
        let value_next = view.direct_value_next as usize as *mut u32;
        let free_head = view.direct_value_free_head as usize as *mut u32;
        let reused_bytes = view.direct_value_reused_bytes as usize as *mut u64;
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        // SAFETY: runtime publication owns these stable counters and the
        // request executes synchronously on one thread.
        unsafe {
            let index = *free_head;
            if index != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE {
                *free_head = (*slots.add(index as usize)).payload as u32;
                *reused_bytes = (*reused_bytes)
                    .saturating_add(std::mem::size_of::<php_jit::JitNativeValueSlot>() as u64);
                return Ok(index);
            }
            let index = *value_next;
            if index as usize >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
                return Err("direct native value arena exhausted");
            }
            *value_next = index + 1;
            Ok(index)
        }
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn reserve_direct_string_range(&mut self, length: usize) -> Option<(usize, u32)> {
        let capacity = length
            .max(php_jit::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY as usize)
            .checked_next_power_of_two()?;
        let capacity = u32::try_from(capacity).ok()?;
        let bucket = capacity.trailing_zeros() as usize;
        let view = self.header.active_runtime_view();
        let heads = view.direct_string_free_heads as usize as *mut u32;
        let bytes = view.direct_string_bytes as usize as *mut u8;
        let next = view.direct_string_next as usize as *mut u32;
        let reused_bytes = view.direct_string_reused_bytes as usize as *mut u64;
        unsafe {
            let head = *heads.add(bucket);
            if head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE {
                let previous = (bytes.add(head as usize) as *const u32).read_unaligned();
                *heads.add(bucket) = previous;
                *reused_bytes = (*reused_bytes).saturating_add(u64::from(capacity));
                return Some((head as usize, capacity));
            }
            let start = *next;
            let end = start.checked_add(capacity)?;
            if end as usize > php_jit::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY {
                return None;
            }
            *next = end;
            Some((start as usize, capacity))
        }
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn free_direct_string_range(&mut self, start: usize, capacity: u32) {
        if capacity < php_jit::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY || !capacity.is_power_of_two()
        {
            return;
        }
        let view = self.header.active_runtime_view();
        let bucket = capacity.trailing_zeros() as usize;
        let heads = view.direct_string_free_heads as usize as *mut u32;
        let bytes = view.direct_string_bytes as usize as *mut u8;
        unsafe {
            let previous = *heads.add(bucket);
            (bytes.add(start) as *mut u32).write_unaligned(previous);
            *heads.add(bucket) =
                u32::try_from(start).unwrap_or(php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE);
        }
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn reserve_direct_array_range(&mut self, length: usize) -> Result<(usize, u32), &'static str> {
        let capacity = length.max(1).next_power_of_two();
        let capacity_u32 =
            u32::try_from(capacity).map_err(|_| "direct native array capacity overflow")?;
        let bucket = capacity.trailing_zeros() as usize;
        if bucket >= php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS {
            return Err("direct native array capacity exceeded its stable arena");
        }
        let view = self.header.active_runtime_view();
        let heads = view.direct_array_free_heads as usize as *mut u32;
        let entries = view.direct_array_entries as usize as *mut php_jit::JitNativeDirectArrayEntry;
        let next = view.direct_array_next as usize as *mut u32;
        let reused_bytes = view.direct_array_reused_bytes as usize as *mut u64;
        unsafe {
            let head = *heads.add(bucket);
            if head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE {
                let start = head as usize;
                *heads.add(bucket) = (*entries.add(start)).key as u32;
                *reused_bytes = (*reused_bytes).saturating_add(
                    capacity
                        .saturating_mul(std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>())
                        as u64,
                );
                return Ok((start, capacity_u32));
            }
            let start = *next;
            let end = start
                .checked_add(capacity_u32)
                .ok_or("direct native array range overflow")?;
            if end as usize > php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY {
                return Err("direct native array arena exhausted");
            }
            *next = end;
            Ok((start as usize, capacity_u32))
        }
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn free_direct_array_range(&mut self, start: usize, capacity: u32) {
        if capacity == 0 || !capacity.is_power_of_two() {
            return;
        }
        let bucket = capacity.trailing_zeros() as usize;
        if bucket >= php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS
            || start >= php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
        {
            return;
        }
        let view = self.header.active_runtime_view();
        let heads = view.direct_array_free_heads as usize as *mut u32;
        let entries = view.direct_array_entries as usize as *mut php_jit::JitNativeDirectArrayEntry;
        unsafe {
            let previous = *heads.add(bucket);
            *entries.add(start) = php_jit::JitNativeDirectArrayEntry {
                key: i64::from(previous),
                value: 0,
            };
            *heads.add(bucket) =
                u32::try_from(start).unwrap_or(php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE);
        }
    }

    /// Publishes one exact-handler result directly into the request-owned
    /// native string/value plane. Publication metadata guarantees every
    /// pointer in the runtime view; this path performs only PHP-visible arena
    /// bounds checks and never recovers the cold execution coordinator.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    pub(crate) fn publish_direct_string_bytes(
        &mut self,
        bytes: &[u8],
    ) -> Result<i64, &'static str> {
        self.publish_direct_string_with(bytes.len(), |output| output.copy_from_slice(bytes))
    }

    /// Reserves and fills one authoritative native string in place.
    ///
    /// Exact operations use this boundary when the result is computed from
    /// existing native byte ranges. The result is never materialized as an
    /// intermediate Rust-owned byte vector.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn publish_direct_string_with(
        &mut self,
        length: usize,
        fill: impl FnOnce(&mut [u8]),
    ) -> Result<i64, &'static str> {
        self.try_publish_direct_string_with(length, |output| {
            fill(output);
            Ok(())
        })
    }

    /// Reserves, fills, and publishes one native string transactionally.
    ///
    /// A failed renderer returns both reservations to their native free lists;
    /// no partially initialized string can become visible to generated code.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn try_publish_direct_string_with(
        &mut self,
        length: usize,
        fill: impl FnOnce(&mut [u8]) -> Result<(), &'static str>,
    ) -> Result<i64, &'static str> {
        self.try_publish_direct_string_with_capacity(length, |output| {
            fill(output)?;
            Ok(length)
        })
        .map_err(|error| match error {
            NativeDirectStringPublishError::Arena(error)
            | NativeDirectStringPublishError::Fill(error) => error,
        })
    }

    /// Publishes the initialized prefix of a bounded, fallible native writer.
    ///
    /// The slot retains the complete reservation capacity for correct arena
    /// release while exposing only the writer's actual PHP string length.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn try_publish_direct_string_with_capacity<E>(
        &mut self,
        maximum_length: usize,
        fill: impl FnOnce(&mut [u8]) -> Result<usize, E>,
    ) -> Result<i64, NativeDirectStringPublishError<E>> {
        let view = self.header.active_runtime_view();
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        let string_bytes = view.direct_string_bytes as usize as *mut u8;

        // SAFETY: `activate_native_context` publishes stable request-owned
        // arena bases and counters before generated code can invoke an exact
        // handler. A native request is single-threaded while this state is
        // active, so reservation and publication are one ordered operation.
        let (start, capacity) = self.reserve_direct_string_range(maximum_length).ok_or(
            NativeDirectStringPublishError::Arena("direct native string arena exhausted"),
        )?;

        let length = unsafe {
            let output = std::slice::from_raw_parts_mut(string_bytes.add(start), maximum_length);
            match fill(output) {
                Ok(length) if length <= maximum_length => length,
                Ok(_) => {
                    self.free_direct_string_range(start, capacity);
                    return Err(NativeDirectStringPublishError::Arena(
                        "direct native string writer exceeded its reservation",
                    ));
                }
                Err(error) => {
                    self.free_direct_string_range(start, capacity);
                    return Err(NativeDirectStringPublishError::Fill(error));
                }
            }
        };

        let index = match self.reserve_direct_value_index() {
            Ok(index) => index,
            Err(error) => {
                self.free_direct_string_range(start, capacity);
                return Err(NativeDirectStringPublishError::Arena(error));
            }
        };
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;

        unsafe {
            let output = std::slice::from_raw_parts_mut(string_bytes.add(start), length);
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                refcount: 1,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
                flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
                reserved: php_jit::jit_native_direct_string_reserved(capacity, output == b"0"),
                payload: length as u64,
                aux: string_bytes.add(start) as usize as u64,
            };
        }
        Ok((php_jit::JIT_VALUE_RUNTIME_STRING_TAG | u64::from(runtime_index)) as i64)
    }

    /// Applies a fixed exact byte transformation directly from one stable
    /// native string into a newly reserved authoritative native string.
    fn publish_direct_string_transform(
        &mut self,
        input: i64,
        output_length: impl FnOnce(&[u8]) -> Option<usize>,
        fill: impl FnOnce(&[u8], &mut [u8]) -> bool,
    ) -> Option<i64> {
        let (input, input_length) = self.stable_native_string_range(input)?;
        // SAFETY: the input owner stays live for this synchronous transform;
        // direct string reservations never relocate another live range.
        #[allow(unsafe_code)]
        let input_bytes = unsafe { std::slice::from_raw_parts(input, input_length) };
        let length = output_length(input_bytes)?;
        self.try_publish_direct_string_with(length, |output| {
            // SAFETY: publication writes to a disjoint freshly reserved range
            // while the source owner and its stable range remain live.
            #[allow(unsafe_code)]
            let input_bytes = unsafe { std::slice::from_raw_parts(input, input_length) };
            fill(input_bytes, output)
                .then_some(())
                .ok_or("native string transform length contract failed")
        })
        .ok()
    }

    fn publish_direct_string_transform2(
        &mut self,
        first: i64,
        second: i64,
        output_length: impl FnOnce(&[u8], &[u8]) -> Option<usize>,
        fill: impl FnOnce(&[u8], &[u8], &mut [u8]) -> bool,
    ) -> Option<i64> {
        let (first, first_length) = self.stable_native_string_range(first)?;
        let (second, second_length) = self.stable_native_string_range(second)?;
        #[allow(unsafe_code)]
        let length = output_length(
            unsafe { std::slice::from_raw_parts(first, first_length) },
            unsafe { std::slice::from_raw_parts(second, second_length) },
        )?;
        self.try_publish_direct_string_with(length, |output| {
            #[allow(unsafe_code)]
            let (first, second) = unsafe {
                (
                    std::slice::from_raw_parts(first, first_length),
                    std::slice::from_raw_parts(second, second_length),
                )
            };
            fill(first, second, output)
                .then_some(())
                .ok_or("native two-string transform length contract failed")
        })
        .ok()
    }

    fn publish_direct_string_transform3(
        &mut self,
        first: i64,
        second: i64,
        third: i64,
        output_length: impl FnOnce(&[u8], &[u8], &[u8]) -> Option<usize>,
        fill: impl FnOnce(&[u8], &[u8], &[u8], &mut [u8]) -> bool,
    ) -> Option<i64> {
        let (first, first_length) = self.stable_native_string_range(first)?;
        let (second, second_length) = self.stable_native_string_range(second)?;
        let (third, third_length) = self.stable_native_string_range(third)?;
        #[allow(unsafe_code)]
        let length = output_length(
            unsafe { std::slice::from_raw_parts(first, first_length) },
            unsafe { std::slice::from_raw_parts(second, second_length) },
            unsafe { std::slice::from_raw_parts(third, third_length) },
        )?;
        self.try_publish_direct_string_with(length, |output| {
            #[allow(unsafe_code)]
            let (first, second, third) = unsafe {
                (
                    std::slice::from_raw_parts(first, first_length),
                    std::slice::from_raw_parts(second, second_length),
                    std::slice::from_raw_parts(third, third_length),
                )
            };
            fill(first, second, third, output)
                .then_some(())
                .ok_or("native three-string transform length contract failed")
        })
        .ok()
    }

    /// Publishes an exact-handler floating-point result directly into the
    /// authoritative request arena.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn publish_direct_float(&mut self, value: f64) -> Result<i64, &'static str> {
        let index = self.reserve_direct_value_index()?;
        let view = self.header.active_runtime_view();
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        unsafe {
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                refcount: 1,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT,
                payload: value.to_bits(),
                ..php_jit::JitNativeValueSlot::default()
            };
        }
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
        Ok((php_jit::JIT_VALUE_RUNTIME_FLOAT_TAG | u64::from(runtime_index)) as i64)
    }

    /// Keeps exact-handler integer results immediate unless their bit pattern
    /// overlaps a native handle namespace.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    pub(crate) fn publish_direct_int(&mut self, value: i64) -> Result<i64, &'static str> {
        if php_jit::jit_decode_runtime_value(value).is_none()
            && php_jit::jit_decode_constant(value).is_none()
        {
            return Ok(value);
        }
        let index = self.reserve_direct_value_index()?;
        let view = self.header.active_runtime_view();
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        unsafe {
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                refcount: 1,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT,
                flags: php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION,
                payload: value as u64,
                ..php_jit::JitNativeValueSlot::default()
            };
        }
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
        Ok(php_jit::jit_encode_runtime_value(runtime_index))
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    pub(crate) fn direct_slot(&self, encoded: i64) -> Option<(usize, php_jit::JitNativeValueSlot)> {
        let runtime_index = php_jit::jit_decode_runtime_value(encoded)?;
        let index =
            runtime_index.checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)? as usize;
        if index >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
            return None;
        }
        let view = self.header.active_runtime_view();
        let slots = view.direct_value_slots as usize as *const php_jit::JitNativeValueSlot;
        let slot = unsafe { *slots.add(index) };
        (slot.refcount != 0).then_some((index, slot))
    }

    /// Resolves PHP by-value reads on the authoritative native plane.
    ///
    /// Exact handlers borrow their arguments and must observe a direct
    /// reference's payload without materializing a `ReferenceCell` or
    /// acquiring a second owner. Publication guarantees the direct-reference
    /// layout; a compatibility reference remains an exact baseline boundary.
    fn native_by_value_encoding(&self, mut encoded: i64) -> Option<i64> {
        for _ in 0..64 {
            let Some((_, slot)) = self.direct_slot(encoded) else {
                return Some(encoded);
            };
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                && native_reference_state(slot.reserved)
                    != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
            {
                encoded = slot.payload as i64;
                continue;
            }
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR {
                return None;
            }
            return Some(encoded);
        }
        None
    }

    /// Returns the stable backing owner for one authoritative direct object.
    /// The owner pointer is slot-parallel and is cleared before the slot is
    /// recycled, so exact object operations need no request hash lookup.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    pub(crate) fn direct_object(&self, encoded: i64) -> Option<&php_runtime::api::ObjectRef> {
        let encoded = self.native_by_value_encoding(encoded)?;
        let (index, slot) = self.direct_slot(encoded)?;
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT {
            return None;
        }
        let owners = self.header.active_runtime_view().direct_object_owners as usize as *const u64;
        // SAFETY: both arrays share the direct-value capacity and stable
        // request lifetime. A nonzero owner is a Box<ObjectRef> published
        // before the direct object descriptor becomes visible.
        let owner = unsafe { *owners.add(index) } as usize as *const php_runtime::api::ObjectRef;
        unsafe { owner.as_ref() }
    }

    /// Returns PHP's exact `gettype()` or `get_debug_type()` name for one
    /// authoritative native value without constructing or decoding a
    /// compatibility `Value`.
    pub(crate) fn exact_type_name(&self, encoded: i64, debug: bool) -> Option<Vec<u8>> {
        let selected = |debug_name: &[u8], ordinary_name: &[u8]| {
            if debug { debug_name } else { ordinary_name }.to_vec()
        };
        let encoded = self.native_by_value_encoding(encoded)?;
        if let Some(constant) = php_jit::jit_decode_constant(encoded) {
            return match constant {
                u32::MAX | php_jit::JIT_VALUE_UNINITIALIZED => Some(selected(b"null", b"NULL")),
                php_jit::JIT_VALUE_FALSE | php_jit::JIT_VALUE_TRUE => {
                    Some(selected(b"bool", b"boolean"))
                }
                _ => None,
            };
        }
        let Some((_, slot)) = self.direct_slot(encoded) else {
            return (php_jit::jit_decode_runtime_value(encoded).is_none())
                .then(|| selected(b"int", b"integer"));
        };
        match slot.kind {
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT => Some(selected(b"int", b"integer")),
            php_jit::JIT_NATIVE_VALUE_VIEW_STRING => Some(b"string".to_vec()),
            php_jit::JIT_NATIVE_VALUE_VIEW_ARRAY
            | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
            | php_jit::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
            | php_jit::JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY
            | php_jit::JIT_NATIVE_VALUE_VIEW_GLOBALS_PROXY => Some(b"array".to_vec()),
            php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => Some(selected(b"float", b"double")),
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT => {
                if debug {
                    self.direct_object(encoded)
                        .map(|object| object.display_name().into_bytes())
                } else {
                    Some(b"object".to_vec())
                }
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE => {
                Some(selected(b"Closure", b"object"))
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
            | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER => {
                Some(selected(b"Fiber", b"object"))
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR
            | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_GENERATOR
            | php_jit::JIT_NATIVE_VALUE_VIEW_COLD_GENERATOR => {
                Some(selected(b"Generator", b"object"))
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE => {
                let resource = self.native_resource_view(encoded)?;
                if debug && resource.is_open() {
                    Some(format!("resource ({})", resource.resource_type()).into_bytes())
                } else if resource.is_open() {
                    Some(b"resource".to_vec())
                } else {
                    Some(b"resource (closed)".to_vec())
                }
            }
            _ => None,
        }
    }

    /// Reads PHP's stable object identity from the authoritative native owner.
    ///
    /// Direct object descriptors repurpose their payload for the published
    /// property-layout id, so identity must come from the slot-parallel stable
    /// owner. Prepared closures keep their object id in the same request-owned
    /// callable record. Other callable forms are not PHP objects and request
    /// the one baseline continuation for the proper TypeError.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_object_identity(&self, encoded: i64) -> Option<u64> {
        let encoded = self.native_by_value_encoding(encoded)?;
        let (_, slot) = self.direct_slot(encoded)?;
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT {
            return self.direct_object(encoded).map(|object| object.id());
        }
        let callable = self.native_prepared_callable_view(encoded)?;
        (callable.kind == php_jit::JIT_NATIVE_CALLABLE_KIND_CLOSURE).then_some(slot.payload)
    }

    /// Borrows the authoritative C-stable metadata prefix of one prepared
    /// callable. Exact code uses this view directly; the adjacent Rust enum is
    /// a baseline/cold compatibility sidecar and is intentionally unreachable
    /// through this capability.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_prepared_callable_view(
        &self,
        encoded: i64,
    ) -> Option<&php_jit::JitNativePreparedCallableView> {
        let (_, slot) = self.direct_slot(encoded)?;
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
            || slot.flags != php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION
            || slot.aux == 0
        {
            return None;
        }
        unsafe { (slot.aux as usize as *const php_jit::JitNativePreparedCallableView).as_ref() }
    }

    /// Copies an exact dynamic-property name from authoritative native string
    /// storage. Source-unit constants are stabilized by optimizing lowering
    /// before entering this ABI; every other shape requests baseline.
    fn exact_dynamic_property_name(&self, encoded: i64) -> Option<&str> {
        std::str::from_utf8(self.native_string_view(encoded)?).ok()
    }

    /// Resolves the stable cell backing one undeclared property.
    ///
    /// Existing cells are admitted for every exact native object. A missing
    /// name reserves an uninitialized tombstone only when the descriptor's
    /// published class capability permits dynamic properties.
    pub(crate) fn exact_dynamic_property_slot_location(
        &self,
        object: i64,
        property: &str,
    ) -> Option<*mut php_runtime::api::NativeDeclaredPropertySlot> {
        let object = self.native_by_value_encoding(object)?;
        let (_, descriptor) = self.direct_slot(object)?;
        if descriptor.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
            || !php_jit::jit_native_object_property_view_is_published(descriptor.flags)
        {
            return None;
        }
        let owner = self.direct_object(object)?;
        if let Some(slot) =
            owner.native_public_declared_property_slot_location(descriptor.payload, property)
        {
            return Some(slot);
        }
        match owner.native_dynamic_property_slot_location(descriptor.payload, property)? {
            Some(slot) => Some(slot),
            None
                if descriptor.flags & php_jit::JIT_NATIVE_OBJECT_ALLOWS_DYNAMIC_PROPERTIES != 0 =>
            {
                owner.ensure_native_dynamic_property_slot_location(descriptor.payload, property)
            }
            None => None,
        }
    }

    /// Resolves a fixed-name dynamic cell whose class/name shape was proven
    /// during publication. Unlike the computed-name entry, this may reserve a
    /// missing cell for a user class carrying `AllowDynamicProperties`.
    pub(crate) fn exact_named_dynamic_property_slot_location(
        &self,
        object: i64,
        property: &str,
    ) -> Option<*mut php_runtime::api::NativeDeclaredPropertySlot> {
        let object = self.native_by_value_encoding(object)?;
        let (_, descriptor) = self.direct_slot(object)?;
        if descriptor.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
            || !php_jit::jit_native_object_property_view_is_published(descriptor.flags)
        {
            return None;
        }
        let owner = self.direct_object(object)?;
        if let Some(slot) =
            owner.native_public_declared_property_slot_location(descriptor.payload, property)
        {
            return Some(slot);
        }
        match owner.native_dynamic_property_slot_location(descriptor.payload, property)? {
            Some(slot) => Some(slot),
            None => {
                owner.ensure_native_dynamic_property_slot_location(descriptor.payload, property)
            }
        }
    }

    /// Resolves the authoritative cell for `isset($o->$name)` / `empty(...)`.
    /// Missing names on classes without `__isset` share one immutable
    /// request-stable absence cell; declared names and magic/unknown classes
    /// take the single baseline continuation for visibility semantics.
    fn exact_dynamic_property_test_slot_location(
        &mut self,
        object: i64,
        property: &str,
    ) -> Option<*mut php_runtime::api::NativeDeclaredPropertySlot> {
        let object = self.native_by_value_encoding(object)?;
        let (_, descriptor) = self.direct_slot(object)?;
        if descriptor.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
            || !php_jit::jit_native_object_property_view_is_published(descriptor.flags)
        {
            return None;
        }
        let owner = self.direct_object(object)?;
        if let Some(slot) =
            owner.native_public_declared_property_slot_location(descriptor.payload, property)
        {
            return Some(slot);
        }
        if owner.native_property_name_is_declared(descriptor.payload, property)? {
            return None;
        }
        if let Some(slot) =
            owner.native_dynamic_property_slot_location(descriptor.payload, property)?
        {
            return Some(slot);
        }
        if owner.native_has_magic_isset()? {
            return None;
        }
        Some(std::ptr::from_mut(&mut self.absent_dynamic_property_slot))
    }

    /// Borrows a direct resource capability, transparently following a
    /// bounded direct-reference chain. The ResourceRef owner is stored in the
    /// slot itself and remains stable through this synchronous exact call.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_resource_view(&self, encoded: i64) -> Option<&php_runtime::api::ResourceRef> {
        let encoded = self.native_by_value_encoding(encoded)?;
        let (_, slot) = self.direct_slot(encoded)?;
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE
            && slot.flags == php_jit::JIT_NATIVE_DIRECT_RESOURCE_ABI_VERSION
            && slot.aux != 0
        {
            let owner = slot.aux as usize as *const php_runtime::api::ResourceRef;
            unsafe { owner.as_ref() }
        } else {
            None
        }
    }

    /// Publishes a freshly created resource into the authoritative direct
    /// plane and records its request identity without constructing `Value`.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn publish_direct_resource(
        &mut self,
        resource: php_runtime::api::ResourceRef,
    ) -> Result<i64, &'static str> {
        let resource_id = resource.id().get();
        let resource_type_length = resource.resource_type().len().max("Unknown".len());
        let resource_type_length = u32::try_from(resource_type_length)
            .map_err(|_| "direct resource type name exceeds the native descriptor")?;
        let handles = unsafe { self.direct_resource_handles.as_mut() }
            .ok_or("direct resource identity table is unavailable")?;
        if let Some(index) = handles.get(&resource_id).copied() {
            let view = self.header.active_runtime_view();
            let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
            let slot = unsafe { &mut *slots.add(index as usize) };
            if slot.refcount == 0
                || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE
                || slot.flags != php_jit::JIT_NATIVE_DIRECT_RESOURCE_ABI_VERSION
                || slot.payload != resource_id
            {
                return Err("direct resource identity points at a dead slot");
            }
            slot.reserved = slot.reserved.max(resource_type_length);
            slot.refcount = slot
                .refcount
                .checked_add(1)
                .ok_or("direct resource refcount overflow")?;
            let runtime_index = index
                .checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .ok_or("direct resource handle overflow")?;
            return Ok(php_jit::jit_encode_typed_runtime_value(
                runtime_index,
                php_jit::JIT_VALUE_RUNTIME_RESOURCE_TAG,
            ));
        }

        let index = self.reserve_direct_value_index()?;
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
        let owner = Box::into_raw(Box::new(resource));
        let slots = self.header.active_runtime_view().direct_value_slots as usize
            as *mut php_jit::JitNativeValueSlot;
        unsafe {
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                // One owner is returned to generated code; the request-wide
                // identity table holds the second until arena recycling.
                // Ordinary SSA release therefore never needs a Rust-drop
                // continuation for this opaque capability.
                refcount: 2,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE,
                flags: php_jit::JIT_NATIVE_DIRECT_RESOURCE_ABI_VERSION,
                reserved: resource_type_length,
                payload: resource_id,
                aux: owner as usize as u64,
            };
        }
        handles.insert(resource_id, index);
        Ok(php_jit::jit_encode_typed_runtime_value(
            runtime_index,
            php_jit::JIT_VALUE_RUNTIME_RESOURCE_TAG,
        ))
    }

    fn stream_context_value_is_native_owned(&self, encoded: i64) -> bool {
        self.stream_context_value_is_native_owned_at(encoded, 0)
    }

    fn stream_context_value_is_native_owned_at(&self, encoded: i64, depth: usize) -> bool {
        if depth > 64 {
            return false;
        }
        let Some((_, slot)) = self.direct_slot(encoded) else {
            if php_jit::jit_decode_runtime_value(encoded).is_some() {
                return false;
            }
            return php_jit::jit_decode_constant(encoded).is_none_or(|constant| {
                matches!(
                    constant,
                    u32::MAX
                        | php_jit::JIT_VALUE_UNINITIALIZED
                        | php_jit::JIT_VALUE_FALSE
                        | php_jit::JIT_VALUE_TRUE
                )
            });
        };
        match slot.kind {
            php_jit::JIT_NATIVE_VALUE_VIEW_STRING
            | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
            | php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => true,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY => self
                .native_direct_array_entries(encoded)
                .is_some_and(|entries| {
                    entries.iter().all(|entry| {
                        self.stream_context_value_is_native_owned_at(entry.key, depth + 1)
                            && self.stream_context_value_is_native_owned_at(entry.value, depth + 1)
                    })
                }),
            _ => false,
        }
    }

    fn native_stream_context_array(&self, encoded: i64) -> Option<i64> {
        let encoded = self.native_by_value_encoding(encoded)?;
        let (_, slot) = self.direct_slot(encoded)?;
        (slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
            && self.stream_context_value_is_native_owned(encoded))
        .then_some(encoded)
    }

    fn duplicate_native_stream_context_array(&mut self, encoded: i64) -> Result<i64, &'static str> {
        let encoded = self
            .native_stream_context_array(encoded)
            .ok_or("stream context options require an authoritative native array")?;
        self.retain_direct_encoded(encoded)?;
        Ok(encoded)
    }

    fn duplicate_native_stream_context_value(&mut self, encoded: i64) -> Result<i64, &'static str> {
        let encoded = self
            .native_by_value_encoding(encoded)
            .ok_or("stream context value requires baseline reference materialization")?;
        if let Some((_, slot)) = self.direct_slot(encoded) {
            return match slot.kind {
                php_jit::JIT_NATIVE_VALUE_VIEW_STRING
                | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
                | php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => {
                    self.retain_direct_encoded(encoded)?;
                    Ok(encoded)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
                    if self.stream_context_value_is_native_owned(encoded) =>
                {
                    self.retain_direct_encoded(encoded)?;
                    Ok(encoded)
                }
                _ => Err("stream context value is outside the native ownership family"),
            };
        }
        if php_jit::jit_decode_runtime_value(encoded).is_some() {
            return Err("stream context value belongs to the cold value plane");
        }
        if let Some(constant) = php_jit::jit_decode_constant(encoded) {
            if matches!(
                constant,
                u32::MAX
                    | php_jit::JIT_VALUE_UNINITIALIZED
                    | php_jit::JIT_VALUE_FALSE
                    | php_jit::JIT_VALUE_TRUE
            ) {
                return Ok(encoded);
            }
            let (bytes, length) = self
                .stable_native_string_range(encoded)
                .ok_or("stream context literal is not a native string")?;
            return self.publish_direct_string_with(length, |output| {
                if length != 0 {
                    // SAFETY: the trusted literal range is immutable for the
                    // active request and publication writes to a disjoint
                    // native string range.
                    #[allow(unsafe_code)]
                    unsafe {
                        std::ptr::copy_nonoverlapping(bytes, output.as_mut_ptr(), length);
                    }
                }
            });
        }
        Ok(encoded)
    }

    fn copy_native_array_with_string_value(
        &mut self,
        array: i64,
        key: &[u8],
        value: i64,
    ) -> Result<i64, &'static str> {
        let array = self
            .native_stream_context_array(array)
            .ok_or("stream context update requires an authoritative native array")?;
        if !self.stream_context_value_is_native_owned(value) {
            return Err("stream context value is outside the native ownership family");
        }
        let (entries, length) = self
            .stable_native_array_range(array)
            .ok_or("stream context array range is unavailable")?;
        let existing = (0..length).find(|index| {
            // SAFETY: the stable range remains live for this synchronous
            // copy and `index` is bounded by its published length.
            #[allow(unsafe_code)]
            let entry = unsafe { *entries.add(*index) };
            self.native_string_view(entry.key)
                .is_some_and(|candidate| candidate == key)
        });
        let output_length = length
            .checked_add(usize::from(existing.is_none()))
            .ok_or("stream context array length overflow")?;
        self.publish_owned_direct_array_with(output_length, |fast, index| {
            if index == length {
                let key = fast.publish_direct_string_bytes(key)?;
                if let Err(error) = fast.retain_direct_encoded(value) {
                    let _ = fast.discard_owned_direct_value(key);
                    return Err(error);
                }
                return Ok(php_jit::JitNativeDirectArrayEntry { key, value });
            }
            // SAFETY: the source arena range is stable across reservations
            // and this index is below its published length.
            #[allow(unsafe_code)]
            let source = unsafe { *entries.add(index) };
            fast.retain_direct_encoded(source.key)?;
            let selected = if existing == Some(index) {
                value
            } else {
                source.value
            };
            if let Err(error) = fast.retain_direct_encoded(selected) {
                let _ = fast.discard_owned_direct_value(source.key);
                return Err(error);
            }
            Ok(php_jit::JitNativeDirectArrayEntry {
                key: source.key,
                value: selected,
            })
        })
    }

    fn native_stream_context_set_named_option(
        &mut self,
        options: i64,
        wrapper: &[u8],
        option: &[u8],
        value: i64,
    ) -> Result<i64, &'static str> {
        let (entries, length) = self
            .stable_native_array_range(options)
            .ok_or("stream context root options are unavailable")?;
        let wrapper_entry = (0..length).find_map(|index| {
            // SAFETY: the stable range remains live for this synchronous
            // lookup and `index` is bounded by its published length.
            #[allow(unsafe_code)]
            let entry = unsafe { *entries.add(index) };
            (self.native_string_view(entry.key) == Some(wrapper)).then_some(entry.value)
        });
        let (wrapper_options, created_wrapper) =
            match wrapper_entry.and_then(|value| self.native_stream_context_array(value)) {
                Some(options) => (options, false),
                None => (
                    self.publish_owned_direct_array_with(0, |_, _| {
                        unreachable!("zero-length native array builder")
                    })?,
                    true,
                ),
            };
        let updated_wrapper =
            match self.copy_native_array_with_string_value(wrapper_options, option, value) {
                Ok(updated) => updated,
                Err(error) => {
                    if created_wrapper {
                        let _ = self.discard_owned_direct_value(wrapper_options);
                    }
                    return Err(error);
                }
            };
        if created_wrapper {
            self.discard_owned_direct_value(wrapper_options)?;
        }
        let updated = self.copy_native_array_with_string_value(options, wrapper, updated_wrapper);
        self.discard_owned_direct_value(updated_wrapper)?;
        updated
    }

    fn merge_native_stream_context_options(
        &mut self,
        current: i64,
        additions: i64,
    ) -> Result<i64, &'static str> {
        let additions = self
            .native_stream_context_array(additions)
            .ok_or("stream context additions require a native array")?;
        let (wrappers, wrapper_count) = self
            .stable_native_array_range(additions)
            .ok_or("stream context additions are unavailable")?;
        let mut merged = self.duplicate_native_stream_context_array(current)?;
        for wrapper_index in 0..wrapper_count {
            // SAFETY: the additions owner remains live for this synchronous
            // merge and the index is bounded by its stable range.
            #[allow(unsafe_code)]
            let wrapper_entry = unsafe { *wrappers.add(wrapper_index) };
            let Some((wrapper, wrapper_length)) =
                self.stable_native_string_range(wrapper_entry.key)
            else {
                self.discard_owned_direct_value(merged)?;
                return Err("stream context wrapper key is not a native string");
            };
            let Some((options, option_count)) = self.stable_native_array_range(wrapper_entry.value)
            else {
                self.discard_owned_direct_value(merged)?;
                return Err("stream context wrapper options are not a native array");
            };
            for option_index in 0..option_count {
                // SAFETY: the nested options owner is part of the still-live
                // additions graph and the index is bounded by its range.
                #[allow(unsafe_code)]
                let option_entry = unsafe { *options.add(option_index) };
                let Some((option, option_length)) =
                    self.stable_native_string_range(option_entry.key)
                else {
                    self.discard_owned_direct_value(merged)?;
                    return Err("stream context option key is not a native string");
                };
                // SAFETY: both stable string ranges remain live through this
                // synchronous copy into newly owned array entries.
                #[allow(unsafe_code)]
                let wrapper = unsafe { std::slice::from_raw_parts(wrapper, wrapper_length) };
                #[allow(unsafe_code)]
                let option = unsafe { std::slice::from_raw_parts(option, option_length) };
                let updated = match self.native_stream_context_set_named_option(
                    merged,
                    wrapper,
                    option,
                    option_entry.value,
                ) {
                    Ok(updated) => updated,
                    Err(error) => {
                        self.discard_owned_direct_value(merged)?;
                        return Err(error);
                    }
                };
                self.discard_owned_direct_value(merged)?;
                merged = updated;
            }
        }
        Ok(merged)
    }

    #[allow(unsafe_code)] // Safety: publication owns the pointed-to state for this request.
    fn native_stream_context_state(&self) -> Option<&NativeStreamContextState> {
        unsafe { self.stream_context.as_ref() }
    }

    #[allow(unsafe_code)] // Safety: exact calls are synchronous and exclusively borrow the request.
    fn native_stream_context_state_mut(&mut self) -> Option<&mut NativeStreamContextState> {
        unsafe { self.stream_context.as_mut() }
    }

    fn native_stream_context_default_options(&self) -> Option<i64> {
        let options = self.native_stream_context_state()?.default_options;
        self.native_stream_context_array(options)
    }

    fn replace_native_stream_context_default_owned(
        &mut self,
        options: i64,
    ) -> Result<(), &'static str> {
        let previous = {
            let state = self
                .native_stream_context_state_mut()
                .ok_or("native stream context state is unavailable")?;
            std::mem::replace(&mut state.default_options, options)
        };
        self.discard_owned_direct_value(previous)
    }

    fn native_stream_context_resource_options(
        &self,
        resource: &php_runtime::api::ResourceRef,
    ) -> Option<i64> {
        if resource.kind() != php_runtime::api::ResourceKind::StreamContext {
            return None;
        }
        self.native_stream_context_state()?
            .resource_options
            .get(&resource.id().get())
            .copied()
            .and_then(|options| self.native_stream_context_array(options))
    }

    fn insert_native_stream_context_resource_owned(
        &mut self,
        resource: &php_runtime::api::ResourceRef,
        options: i64,
    ) -> Result<(), &'static str> {
        if resource.kind() != php_runtime::api::ResourceKind::StreamContext {
            return Err("native stream context resource has the wrong kind");
        }
        let previous = self
            .native_stream_context_state_mut()
            .ok_or("native stream context state is unavailable")?
            .resource_options
            .insert(resource.id().get(), options);
        if let Some(previous) = previous {
            self.discard_owned_direct_value(previous)?;
        }
        Ok(())
    }

    /// Publishes one exact closure record whose captures already own native
    /// encodings. No `Value`, `ReferenceCell`, callsite lookup, or generic
    /// dynamic-code request participates in this allocation.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn publish_prepared_closure_owned(
        &mut self,
        prepared: NativePreparedClosure,
    ) -> Result<i64, &'static str> {
        let rollback = |fast: &mut Self, prepared: &NativePreparedClosure| {
            if let Some(implicit_this) = prepared.implicit_this {
                fast.rollback_direct_retain(implicit_this);
            }
            for capture in prepared.captures.iter().copied() {
                fast.rollback_direct_retain(capture);
            }
        };
        if self.direct_closure_handles.is_null() {
            rollback(self, &prepared);
            return Err("direct closure identity table is unavailable");
        }
        let closure_id = prepared.closure.id;
        if unsafe { (*self.direct_closure_handles).contains_key(&closure_id) } {
            rollback(self, &prepared);
            return Err("new direct closure identity is already published");
        }
        let index = match self.reserve_direct_value_index() {
            Ok(index) => index,
            Err(error) => {
                rollback(self, &prepared);
                return Err(error);
            }
        };
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
        let runtime_view = if self.header.runtime_view_pointer == 0 {
            std::ptr::from_ref(&self.header.runtime_view) as usize as u64
        } else {
            self.header.runtime_view_pointer
        };
        let owner = Box::into_raw(Box::new(NativePreparedCallableOwner::closure(
            prepared,
            runtime_view,
        )));
        let slots = self.header.active_runtime_view().direct_value_slots as usize
            as *mut php_jit::JitNativeValueSlot;
        unsafe {
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                refcount: 1,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE,
                flags: php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION,
                reserved: 0,
                payload: closure_id,
                aux: owner as usize as u64,
            };
            (*self.direct_closure_handles).insert(closure_id, index);
        }
        Ok(php_jit::jit_encode_typed_runtime_value(
            runtime_index,
            php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG,
        ))
    }

    fn exact_callable_value(&self, encoded: i64) -> Option<i64> {
        let encoded = self.native_by_value_encoding(encoded)?;
        (self.direct_slot(encoded).is_some()
            || php_jit::jit_decode_runtime_value(encoded).is_none())
        .then_some(encoded)
    }

    fn exact_callable_array_index(&self, encoded: i64) -> Option<i64> {
        let encoded = self.exact_callable_value(encoded)?;
        if php_jit::jit_decode_runtime_value(encoded).is_none()
            && php_jit::jit_decode_constant(encoded).is_none()
        {
            return Some(encoded);
        }
        let (_, slot) = self.direct_slot(encoded)?;
        (slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
            && slot.flags == php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION)
            .then_some(slot.payload as i64)
    }

    /// Publishes a non-closure callable record directly into the authoritative
    /// native slot plane. A bound object target contributes exactly one child
    /// owner, released with the callable slot.
    fn publish_prepared_callable_owned(
        &mut self,
        owner: NativePreparedCallableOwner,
    ) -> Result<i64, &'static str> {
        let object_target = (owner.native_view.kind
            == php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_OBJECT_METHOD)
            .then_some(owner.native_view.receiver);
        let index = match self.reserve_direct_value_index() {
            Ok(index) => index,
            Err(error) => {
                if let Some(object) = object_target {
                    self.rollback_direct_retain(object);
                }
                return Err(error);
            }
        };
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
        let owner = Box::into_raw(Box::new(owner));
        let slots = self.header.active_runtime_view().direct_value_slots as usize
            as *mut php_jit::JitNativeValueSlot;
        // SAFETY: the reserved direct slot and boxed callable owner remain
        // request-stable until the slot's final native release.
        #[allow(unsafe_code)]
        // Safety: the native request owns every published pointer for the synchronous activation.
        unsafe {
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                refcount: 1,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE,
                flags: php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION,
                reserved: 0,
                payload: 0,
                aux: owner as usize as u64,
            };
        }
        Ok(php_jit::jit_encode_typed_runtime_value(
            runtime_index,
            php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG,
        ))
    }

    fn prepared_callable_fixed_plan(
        &self,
        owner: &NativePreparedCallableOwner,
    ) -> Option<NativeFixedCallablePlan> {
        match owner.native_view.kind {
            php_jit::JIT_NATIVE_CALLABLE_KIND_USER_FUNCTION => {
                let name = std::str::from_utf8(&owner._name_bytes).ok()?;
                self.symbol_query.callable_plan(name)
            }
            php_jit::JIT_NATIVE_CALLABLE_KIND_CLOSURE => {
                let function = php_ir::FunctionId::new(owner.native_view.function_id);
                let compiled = self.symbol_query.active_compiled()?;
                let mut plan = native_fixed_callable_plan(compiled, function, false)?;
                plan.runtime_view = owner.native_view.runtime_view;
                Some(plan)
            }
            php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_OBJECT_METHOD => {
                let method = std::str::from_utf8(&owner._method_bytes).ok()?;
                let object = self.native_query_object(owner.native_view.receiver)?;
                self.symbol_query
                    .method_callable_plan(&object.class_name(), method, true)
            }
            php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_CLASS_METHOD => {
                let class = std::str::from_utf8(&owner._class_bytes).ok()?;
                let method = std::str::from_utf8(&owner._method_bytes).ok()?;
                self.symbol_query.method_callable_plan(class, method, false)
            }
            _ => None,
        }
    }

    #[allow(unsafe_code)] // Safety: callable slots own request-stable boxes for the synchronous query.
    fn native_prepared_callable_owner(&self, encoded: i64) -> Option<&NativePreparedCallableOwner> {
        let (_, slot) = self.direct_slot(encoded)?;
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
            || slot.flags != php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION
            || slot.aux == 0
        {
            return None;
        }
        unsafe { (slot.aux as usize as *const NativePreparedCallableOwner).as_ref() }
    }

    fn direct_callable_array_parts(&self, encoded: i64) -> Option<(i64, i64)> {
        let entries = self.native_direct_array_entries(encoded)?;
        if entries.len() != 2 {
            return None;
        }
        let mut target = None;
        let mut method = None;
        for entry in entries {
            match self.exact_callable_array_index(entry.key) {
                Some(0) if target.is_none() => target = self.exact_callable_value(entry.value),
                Some(1) if method.is_none() => method = self.exact_callable_value(entry.value),
                _ => return None,
            }
        }
        Some((target?, method?))
    }

    /// Queries PHP callability directly against authoritative native values.
    ///
    /// Unpublished class metadata and visibility-sensitive methods return
    /// `None`; the caller then takes its one baseline continuation before the
    /// optional output reference is modified.
    fn direct_callable_is_valid(&self, encoded: i64, syntax_only: bool) -> Option<bool> {
        let encoded = self.native_by_value_encoding(encoded)?;
        if self.native_prepared_callable_view(encoded).is_some() {
            return Some(true);
        }
        if let Some(name) = self.native_string_view(encoded) {
            if syntax_only {
                return Some(true);
            }
            let Ok(name) = std::str::from_utf8(name) else {
                return Some(false);
            };
            if let Some((class, method)) = name.split_once("::") {
                let class = class.trim_start_matches('\\');
                if class.is_empty() {
                    return Some(false);
                }
                return self.symbol_query.method_is_callable(class, method, false);
            }
            let name = name.trim_start_matches('\\');
            return Some(!name.is_empty() && self.symbol_query.function_exists(name));
        }
        if let Some(object) = self.direct_object(encoded) {
            return self
                .symbol_query
                .method_is_callable(&object.class_name(), "__invoke", true);
        }
        if let Some((_, slot)) = self.direct_slot(encoded) {
            if matches!(
                slot.kind,
                php_jit::JIT_NATIVE_VALUE_VIEW_ARRAY | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
            ) {
                let Some((target, method)) = self.direct_callable_array_parts(encoded) else {
                    return Some(false);
                };
                let Some(method) = self.native_string_view(method) else {
                    return Some(false);
                };
                if syntax_only {
                    return Some(
                        self.native_string_view(target).is_some()
                            || self.direct_object(target).is_some(),
                    );
                }
                let Ok(method) = std::str::from_utf8(method) else {
                    return Some(false);
                };
                if let Some(object) = self.direct_object(target) {
                    return self.symbol_query.method_is_callable(
                        &object.class_name(),
                        method,
                        true,
                    );
                }
                let Some(class) = self.native_string_view(target) else {
                    return Some(false);
                };
                let Ok(class) = std::str::from_utf8(class) else {
                    return Some(false);
                };
                let class = class.trim_start_matches('\\');
                if class.is_empty() {
                    return Some(false);
                }
                return self.symbol_query.method_is_callable(class, method, false);
            }
            return Some(false);
        }
        if php_jit::jit_decode_runtime_value(encoded).is_some() {
            return None;
        }
        Some(false)
    }

    fn exact_callback_handler_integer(&self, encoded: i64) -> Option<i64> {
        let encoded = self.native_by_value_encoding(encoded)?;
        if php_jit::jit_decode_runtime_value(encoded).is_none()
            && php_jit::jit_decode_constant(encoded).is_none()
        {
            return Some(encoded);
        }
        let (_, slot) = self.direct_slot(encoded)?;
        (slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
            && slot.flags == php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION)
            .then_some(slot.payload as i64)
    }

    #[allow(unsafe_code)]
    fn mark_exact_callback_roots_dirty(&mut self) {
        let pending = self.header.active_runtime_view().root_mutation_pending as usize as *mut u32;
        if !pending.is_null() {
            unsafe {
                *pending = 1;
            }
        }
    }

    #[allow(unsafe_code)]
    fn exact_set_error_handler(&mut self, callback: i64, levels: i64) -> Option<i64> {
        let callback = self.native_by_value_encoding(callback)?;
        if !self.direct_callable_is_valid(callback, false)? {
            return None;
        }
        let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
        let levels = if levels == missing {
            -1
        } else {
            self.exact_callback_handler_integer(levels)?
        };
        let state = unsafe { self.callback_handlers.as_ref()? };
        let previous = state
            .error_handlers
            .last()
            .map(|handler| handler.callback)
            .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX));
        self.retain_direct_encoded(previous).ok()?;
        if self.retain_direct_encoded(callback).is_err() {
            let _ = self.discard_owned_direct_value(previous);
            return None;
        }
        unsafe {
            self.callback_handlers
                .as_mut()?
                .error_handlers
                .push(NativeRegisteredErrorHandler { callback, levels });
        }
        self.mark_exact_callback_roots_dirty();
        Some(previous)
    }

    #[allow(unsafe_code)]
    fn exact_restore_error_handler(&mut self) -> Option<i64> {
        let callback = unsafe {
            self.callback_handlers
                .as_ref()?
                .error_handlers
                .last()
                .map(|handler| handler.callback)
        };
        if let Some(callback) = callback
            && !self.direct_owner_is_fast_discardable(callback)
        {
            return None;
        }
        if let Some(handler) = unsafe { self.callback_handlers.as_mut()?.error_handlers.pop() } {
            let released = self.discard_owned_direct_value(handler.callback);
            debug_assert!(released.is_ok());
            self.mark_exact_callback_roots_dirty();
        }
        Some(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE))
    }

    #[allow(unsafe_code)]
    fn exact_set_exception_handler(&mut self, callback: i64) -> Option<i64> {
        let callback = self.native_by_value_encoding(callback)?;
        if !self.direct_callable_is_valid(callback, false)? {
            return None;
        }
        let state = unsafe { self.callback_handlers.as_ref()? };
        let previous = state
            .exception_handlers
            .last()
            .copied()
            .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX));
        self.retain_direct_encoded(previous).ok()?;
        if self.retain_direct_encoded(callback).is_err() {
            let _ = self.discard_owned_direct_value(previous);
            return None;
        }
        unsafe {
            self.callback_handlers
                .as_mut()?
                .exception_handlers
                .push(callback);
        }
        self.mark_exact_callback_roots_dirty();
        Some(previous)
    }

    #[allow(unsafe_code)]
    fn exact_restore_exception_handler(&mut self) -> Option<i64> {
        let callback = unsafe {
            self.callback_handlers
                .as_ref()?
                .exception_handlers
                .last()
                .copied()
        };
        if let Some(callback) = callback
            && !self.direct_owner_is_fast_discardable(callback)
        {
            return None;
        }
        if let Some(handler) = unsafe { self.callback_handlers.as_mut()?.exception_handlers.pop() }
        {
            let released = self.discard_owned_direct_value(handler);
            debug_assert!(released.is_ok());
            self.mark_exact_callback_roots_dirty();
        }
        Some(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE))
    }

    #[allow(unsafe_code)]
    fn exact_get_exception_handler(&mut self) -> Option<i64> {
        let handler = unsafe {
            self.callback_handlers
                .as_ref()?
                .exception_handlers
                .last()
                .copied()
        }
        .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX));
        self.retain_direct_encoded(handler).ok()?;
        Some(handler)
    }

    fn direct_registered_callbacks_equal(&self, left: i64, right: i64) -> bool {
        let Some(left) = self.native_by_value_encoding(left) else {
            return false;
        };
        let Some(right) = self.native_by_value_encoding(right) else {
            return false;
        };
        if left == right {
            return true;
        }
        if let (Some(left), Some(right)) = (
            self.native_string_view(left),
            self.native_string_view(right),
        ) {
            return left == right;
        }
        if let (Some(left), Some(right)) = (self.direct_object(left), self.direct_object(right)) {
            return left.id() == right.id();
        }
        if let (Some((left_target, left_method)), Some((right_target, right_method))) = (
            self.direct_callable_array_parts(left),
            self.direct_callable_array_parts(right),
        ) {
            return self.direct_registered_callbacks_equal(left_target, right_target)
                && self.direct_registered_callbacks_equal(left_method, right_method);
        }
        let (Some(left), Some(right)) = (
            self.native_prepared_callable_owner(left),
            self.native_prepared_callable_owner(right),
        ) else {
            return false;
        };
        let left = left.native_view;
        let right = right.native_view;
        if left.kind != right.kind {
            return false;
        }
        match left.kind {
            php_jit::JIT_NATIVE_CALLABLE_KIND_CLOSURE => false,
            php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_OBJECT_METHOD => {
                self.direct_registered_callbacks_equal(left.receiver, right.receiver)
                    && native_callable_view_bytes_equal(
                        left.method_bytes,
                        left.method_length,
                        right.method_bytes,
                        right.method_length,
                    )
            }
            php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_CLASS_METHOD => {
                native_callable_view_bytes_equal(
                    left.class_bytes,
                    left.class_length,
                    right.class_bytes,
                    right.class_length,
                ) && native_callable_view_bytes_equal(
                    left.method_bytes,
                    left.method_length,
                    right.method_bytes,
                    right.method_length,
                )
            }
            _ => native_callable_view_bytes_equal(
                left.name_bytes,
                left.name_length,
                right.name_bytes,
                right.name_length,
            ),
        }
    }

    #[allow(unsafe_code)]
    fn exact_register_autoload_callback(&mut self, callback: i64, prepend: bool) -> Option<i64> {
        let callback = self.native_by_value_encoding(callback)?;
        if !self.direct_callable_is_valid(callback, false)? {
            return None;
        }
        self.retain_direct_encoded(callback).ok()?;
        let registered = NativeRegisteredAutoloadCallback {
            callable: callback,
            transient_export: self.callback_transient_export != 0,
        };
        let state = unsafe { self.callback_handlers.as_mut()? };
        if prepend {
            state.autoload_callbacks.insert(0, registered);
        } else {
            state.autoload_callbacks.push(registered);
        }
        self.mark_exact_callback_roots_dirty();
        Some(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE))
    }

    #[allow(unsafe_code)]
    fn exact_unregister_autoload_callback(&mut self, callback: i64) -> Option<i64> {
        let callback = self.native_by_value_encoding(callback)?;
        let state = unsafe { self.callback_handlers.as_ref()? };
        if state.autoload_callbacks.iter().any(|candidate| {
            self.direct_registered_callbacks_equal(candidate.callable, callback)
                && !self.direct_owner_is_fast_discardable(candidate.callable)
        }) {
            return None;
        }
        let callbacks =
            std::mem::take(&mut unsafe { self.callback_handlers.as_mut()? }.autoload_callbacks);
        let mut retained = Vec::with_capacity(callbacks.len());
        let mut removed = Vec::new();
        for candidate in callbacks {
            if self.direct_registered_callbacks_equal(candidate.callable, callback) {
                removed.push(candidate);
            } else {
                retained.push(candidate);
            }
        }
        unsafe { self.callback_handlers.as_mut()? }.autoload_callbacks = retained;
        let changed = !removed.is_empty();
        for callback in removed {
            let released = self.discard_owned_direct_value(callback.callable);
            debug_assert!(released.is_ok());
        }
        if changed {
            self.mark_exact_callback_roots_dirty();
        }
        Some(php_jit::jit_encode_constant(if changed {
            php_jit::JIT_VALUE_TRUE
        } else {
            php_jit::JIT_VALUE_FALSE
        }))
    }

    #[allow(unsafe_code)]
    fn exact_autoload_functions(&mut self) -> Option<i64> {
        let callbacks = unsafe {
            self.callback_handlers
                .as_ref()?
                .autoload_callbacks
                .iter()
                .map(|callback| callback.callable)
                .collect::<Vec<_>>()
        };
        self.publish_owned_direct_array_with(callbacks.len(), |fast, index| {
            let key = fast.publish_direct_int(index as i64)?;
            let callback = callbacks[index];
            if let Err(error) = fast.retain_direct_encoded(callback) {
                let _ = fast.discard_owned_direct_value(key);
                return Err(error);
            }
            Ok(php_jit::JitNativeDirectArrayEntry {
                key,
                value: callback,
            })
        })
        .ok()
    }

    #[allow(unsafe_code)]
    fn exact_register_shutdown_callback(
        &mut self,
        arguments: &[i64],
        function: u32,
        continuation: u32,
    ) -> Option<i64> {
        if self.callback_handlers.is_null() {
            return None;
        }
        let (&callback, arguments) = arguments.split_first()?;
        let callback = self.native_by_value_encoding(callback)?;
        if !self.direct_callable_is_valid(callback, false)? {
            return None;
        }
        let runtime_view = self.header.active_runtime_view();
        let source = self
            .symbol_query
            .compiled_for_runtime_view(&runtime_view)?
            .prepared_continuation_instructions(php_ir::FunctionId::new(function))?
            .get(continuation as usize)?
            .as_ref()?
            .as_ref()
            .clone();
        let mut retained = Vec::with_capacity(arguments.len() + 1);
        let retain = (|| {
            self.retain_direct_encoded(callback)?;
            retained.push(callback);
            for argument in arguments {
                let argument = self.native_by_value_encoding(*argument).ok_or(
                    "shutdown callback argument crossed from the direct native value plane",
                )?;
                self.retain_direct_encoded(argument)?;
                retained.push(argument);
            }
            Ok::<(), &'static str>(())
        })();
        if retain.is_err() {
            for value in retained {
                let released = self.discard_owned_direct_value(value);
                debug_assert!(released.is_ok());
            }
            return None;
        }
        let callback = NativeRegisteredShutdownCallback {
            callable: retained[0],
            arguments: retained[1..].to_vec(),
            source,
            transient_export: self.callback_transient_export != 0,
        };
        unsafe {
            (*self.callback_handlers).shutdown_callbacks.push(callback);
        }
        self.mark_exact_callback_roots_dirty();
        Some(php_jit::jit_encode_constant(u32::MAX))
    }

    /// Formats the optional `is_callable(..., callable_name: ...)` result from
    /// the same native representation inspected by the validity query.
    fn direct_callable_name_bytes(&self, encoded: i64) -> Option<Vec<u8>> {
        let encoded = self.native_by_value_encoding(encoded)?;
        if let Some(scalar) = self.native_printf_scalar(encoded) {
            return Some(match scalar {
                php_runtime::api::NativePrintfScalar::Null => Vec::new(),
                php_runtime::api::NativePrintfScalar::Bool(false) => Vec::new(),
                php_runtime::api::NativePrintfScalar::Bool(true) => b"1".to_vec(),
                php_runtime::api::NativePrintfScalar::Int(value) => value.to_string().into_bytes(),
                php_runtime::api::NativePrintfScalar::Float(value) => {
                    php_runtime::api::float_to_php_string(value).into_bytes()
                }
                php_runtime::api::NativePrintfScalar::String(value) => value.to_vec(),
            });
        }
        if let Some(owner) = self.native_prepared_callable_owner(encoded) {
            return Some(match owner.native_view.kind {
                php_jit::JIT_NATIVE_CALLABLE_KIND_CLOSURE => b"Closure::__invoke".to_vec(),
                php_jit::JIT_NATIVE_CALLABLE_KIND_USER_FUNCTION
                | php_jit::JIT_NATIVE_CALLABLE_KIND_INTERNAL_BUILTIN
                | php_jit::JIT_NATIVE_CALLABLE_KIND_METHOD_PLACEHOLDER
                | php_jit::JIT_NATIVE_CALLABLE_KIND_UNRESOLVED_DYNAMIC => {
                    owner._name_bytes.to_vec()
                }
                php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_OBJECT_METHOD => {
                    let object = self.direct_object(owner.native_view.receiver)?;
                    let mut name = object.display_name().into_bytes();
                    name.extend_from_slice(b"::");
                    name.extend_from_slice(&owner._method_bytes);
                    name
                }
                php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_CLASS_METHOD => {
                    let mut name = owner._class_bytes.to_vec();
                    name.extend_from_slice(b"::");
                    name.extend_from_slice(&owner._method_bytes);
                    name
                }
                _ => return None,
            });
        }
        if let Some(object) = self.direct_object(encoded) {
            let mut name = object.display_name().into_bytes();
            name.extend_from_slice(b"::__invoke");
            return Some(name);
        }
        if let Some((_, slot)) = self.direct_slot(encoded) {
            if matches!(
                slot.kind,
                php_jit::JIT_NATIVE_VALUE_VIEW_ARRAY | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
            ) {
                let Some((target, method)) = self.direct_callable_array_parts(encoded) else {
                    return Some(b"Array".to_vec());
                };
                let Some(method) = self.native_string_view(method) else {
                    return Some(b"Array".to_vec());
                };
                let mut name = if let Some(class) = self.native_string_view(target) {
                    class.to_vec()
                } else if let Some(object) = self.direct_object(target) {
                    object.display_name().into_bytes()
                } else {
                    return Some(b"Array".to_vec());
                };
                name.extend_from_slice(b"::");
                name.extend_from_slice(method);
                return Some(name);
            }
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE {
                let resource = self.native_resource_view(encoded)?;
                return Some(format!("Resource id #{}", resource.id().get()).into_bytes());
            }
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR {
                return Some(b"Generator::__invoke".to_vec());
            }
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER {
                return Some(b"Fiber::__invoke".to_vec());
            }
        }
        None
    }

    /// Acquires every representation-complete callable shape without
    /// constructing a Rust `Value`: an existing prepared callable, a native
    /// function-name string, an invokable object, or a two-entry callable
    /// array. Other PHP shapes take the instruction's one baseline
    /// continuation.
    #[allow(unsafe_code)] // Safety: callable slots own request-stable boxes for the synchronous acquisition.
    pub(crate) fn acquire_direct_callable(
        &mut self,
        encoded: i64,
    ) -> Result<Option<i64>, &'static str> {
        let Some(encoded) = self.exact_callable_value(encoded) else {
            return Ok(None);
        };
        if let Some((_, slot)) = self.direct_slot(encoded) {
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
                && slot.flags == php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION
                && slot.aux != 0
            {
                let owner = slot.aux as usize as *mut NativePreparedCallableOwner;
                // SAFETY: the prepared callable slot owns this request-local
                // box until its final release. Acquisition is synchronous and
                // is the sole publication point that completes a cold record
                // with its immutable same-unit native call contract.
                let fixed_plan = self.prepared_callable_fixed_plan(unsafe { &*owner });
                if let Some(plan) = fixed_plan {
                    unsafe {
                        (*owner).install_fixed_plan(plan);
                    }
                }
                self.retain_direct_encoded(encoded)?;
                return Ok(Some(encoded));
            }
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT {
                let resolved_function = self.native_query_object(encoded).and_then(|object| {
                    self.symbol_query
                        .method_callable_plan(&object.class_name(), "__invoke", true)
                });
                self.retain_direct_encoded(encoded)?;
                return self
                    .publish_prepared_callable_owned(NativePreparedCallableOwner::bound_object(
                        encoded,
                        Box::from(&b"__invoke"[..]),
                        None,
                        resolved_function,
                    ))
                    .map(Some);
            }
        }
        if let Some(name) = self.native_string_view(encoded) {
            let resolved_function = std::str::from_utf8(name)
                .ok()
                .and_then(|name| self.symbol_query.callable_plan(name));
            let name = Box::<[u8]>::from(name);
            if resolved_function.is_none()
                && std::str::from_utf8(&name).ok().is_some_and(|name| {
                    php_std::arginfo::function_metadata_indexed(name.trim_start_matches('\\'))
                        .is_some()
                })
            {
                return self
                    .publish_prepared_callable_owned(NativePreparedCallableOwner::internal_builtin(
                        name,
                    ))
                    .map(Some);
            }
            return self
                .publish_prepared_callable_owned(NativePreparedCallableOwner::user_function(
                    name,
                    resolved_function,
                ))
                .map(Some);
        }
        let Some(entries) = self.native_direct_array_entries(encoded) else {
            return Ok(None);
        };
        let mut target = None;
        let mut method = None;
        for entry in entries {
            match self.exact_callable_array_index(entry.key) {
                Some(0) => target = self.exact_callable_value(entry.value),
                Some(1) => method = self.exact_callable_value(entry.value),
                _ => {}
            }
        }
        let (Some(target), Some(method)) = (target, method) else {
            return Ok(None);
        };
        let Some(method) = self.native_string_view(method).map(Box::<[u8]>::from) else {
            return Ok(None);
        };
        let owner = if self
            .direct_slot(target)
            .is_some_and(|(_, slot)| slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT)
        {
            let resolved_function = std::str::from_utf8(&method).ok().and_then(|method| {
                self.native_query_object(target).and_then(|object| {
                    self.symbol_query
                        .method_callable_plan(&object.class_name(), method, true)
                        .or_else(|| {
                            self.symbol_query
                                .method_callable_plan(&object.class_name(), "__call", true)
                                .map(|mut plan| {
                                    plan.magic_dispatch = true;
                                    plan
                                })
                        })
                })
            });
            self.retain_direct_encoded(target)?;
            NativePreparedCallableOwner::bound_object(target, method, None, resolved_function)
        } else if let Some(class) = self.native_string_view(target).map(Box::<[u8]>::from) {
            let resolved_function = std::str::from_utf8(&class).ok().and_then(|class| {
                std::str::from_utf8(&method).ok().and_then(|method| {
                    self.symbol_query
                        .method_callable_plan(class, method, false)
                        .or_else(|| {
                            self.symbol_query
                                .method_callable_plan(class, "__callStatic", false)
                                .map(|mut plan| {
                                    plan.magic_dispatch = true;
                                    plan
                                })
                        })
                })
            });
            NativePreparedCallableOwner::bound_class(class, method, None, resolved_function)
        } else {
            return Ok(None);
        };
        self.publish_prepared_callable_owned(owner).map(Some)
    }

    /// Resolves a statically named method against an authoritative receiver
    /// and publishes only its immutable generated-entry binding.
    pub(crate) fn acquire_direct_method_callable(
        &mut self,
        target: i64,
        method: &[u8],
        caller_function: u32,
        callback_completed: bool,
    ) -> Result<NativeMethodCallableResolution, &'static str> {
        let Some(target) = self.exact_callable_value(target) else {
            return Ok(NativeMethodCallableResolution::NotFound);
        };
        let method = Box::<[u8]>::from(method);
        let owner = if self
            .direct_slot(target)
            .is_some_and(|(_, slot)| slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT)
        {
            let active_view = self.header.active_runtime_view();
            let root_runtime_view = std::ptr::from_ref(&self.header.runtime_view) as usize as u64;
            let exact_reflection = self.direct_object(target).and_then(|object| {
                object
                    .class_name()
                    .eq_ignore_ascii_case("ReflectionClass")
                    .then(|| {
                        if method.eq_ignore_ascii_case(b"__construct") {
                            Some((
                                1,
                                crate::native_exact::jit_native_reflection_class_construct_php_entry
                                    as *const () as usize as u64,
                                false,
                            ))
                        } else if method.eq_ignore_ascii_case(b"getName") {
                            Some((
                                0,
                                crate::native_exact::jit_native_reflection_class_get_name_php_entry
                                    as *const () as usize as u64,
                                true,
                            ))
                        } else if method.eq_ignore_ascii_case(b"hasProperty") {
                            Some((
                                1,
                                crate::native_exact::jit_native_reflection_class_has_property_php_entry
                                    as *const () as usize as u64,
                                false,
                            ))
                        } else {
                            None
                        }
                    })
                    .flatten()
            });
            let resolved_function = std::str::from_utf8(&method).ok().and_then(|method| {
                self.native_query_object(target).and_then(|object| {
                    self.symbol_query
                        .scoped_method_callable_plan(
                            &object.class_name(),
                            method,
                            true,
                            caller_function,
                            &active_view,
                            root_runtime_view,
                        )
                        .or_else(|| {
                            self.symbol_query
                                .scoped_method_callable_plan(
                                    &object.class_name(),
                                    "__call",
                                    true,
                                    caller_function,
                                    &active_view,
                                    root_runtime_view,
                                )
                                .map(|mut plan| {
                                    plan.magic_dispatch = true;
                                    plan
                                })
                        })
                })
            });
            self.retain_direct_encoded(target)?;
            if let Some((visible_arity, direct_entry, returns_string)) = exact_reflection {
                NativePreparedCallableOwner::exact_bound_object(
                    target,
                    method,
                    visible_arity,
                    direct_entry,
                    returns_string,
                )
            } else {
                NativePreparedCallableOwner::bound_object(target, method, None, resolved_function)
            }
        } else if let Some(class) = self.native_string_view(target).map(Box::<[u8]>::from) {
            let active_view = self.header.active_runtime_view();
            let root_runtime_view = std::ptr::from_ref(&self.header.runtime_view) as usize as u64;
            let resolved_function = std::str::from_utf8(&class).ok().and_then(|class| {
                std::str::from_utf8(&method).ok().and_then(|method| {
                    self.symbol_query
                        .scoped_method_callable_plan(
                            class,
                            method,
                            false,
                            caller_function,
                            &active_view,
                            root_runtime_view,
                        )
                        .or_else(|| {
                            self.symbol_query
                                .scoped_method_callable_plan(
                                    class,
                                    "__callStatic",
                                    false,
                                    caller_function,
                                    &active_view,
                                    root_runtime_view,
                                )
                                .map(|mut plan| {
                                    plan.magic_dispatch = true;
                                    plan
                                })
                        })
                })
            });
            if resolved_function.is_none()
                && std::str::from_utf8(&class)
                    .ok()
                    .is_some_and(|class| self.symbol_query.class_handle(class).is_none())
            {
                match self.acquire_direct_class_plan(target, callback_completed)? {
                    NativeClassPlanResolution::Ready(_) => {}
                    NativeClassPlanResolution::InvokeUserCallback(callback) => {
                        return Ok(NativeMethodCallableResolution::InvokeUserCallback(callback));
                    }
                    NativeClassPlanResolution::NotFound => {
                        return Ok(NativeMethodCallableResolution::NotFound);
                    }
                }
            }
            if let Some(plan) = resolved_function {
                self.publish_late_static_constant_sites(&class, plan)?;
            }
            NativePreparedCallableOwner::bound_class(class, method, None, resolved_function)
        } else {
            return Ok(NativeMethodCallableResolution::NotFound);
        };
        self.publish_prepared_callable_owned(owner)
            .map(NativeMethodCallableResolution::Ready)
    }

    /// Publishes direct literal owners for `static::CONST` sites in one
    /// already-resolved generated method. The method-acquisition boundary has
    /// fixed both the called class and target runtime view, so the callee can
    /// consume its ordinary numeric constant slot with no per-instruction
    /// lookup or compatibility value conversion.
    #[allow(unsafe_code)] // Safety: published runtime views and their slot arenas are request-stable.
    fn publish_late_static_constant_sites(
        &mut self,
        called_class: &[u8],
        plan: NativeFixedCallablePlan,
    ) -> Result<(), &'static str> {
        let called_class = std::str::from_utf8(called_class)
            .map(php_ir::module::normalize_class_name)
            .map_err(|_| "late-static called class is not UTF-8")?;
        let view_pointer = usize::try_from(plan.runtime_view)
            .ok()
            .filter(|pointer| *pointer != 0)
            .ok_or("late-static callable has no published runtime view")?
            as *mut php_jit::JitNativeRuntimeView;
        let view =
            unsafe { view_pointer.as_ref() }.ok_or("late-static callable runtime view is null")?;
        let compiled = self
            .symbol_query
            .compiled_for_runtime_view(view)
            .ok_or("late-static callable compiled unit is unavailable")?;
        let instructions = compiled
            .prepared_continuation_instructions(plan.function)
            .ok_or("late-static callable continuation metadata is unavailable")?;
        let function_offsets = view.trusted_property_function_offsets as usize as *const u32;
        let literal_slots =
            view.trusted_literal_slots as usize as *const php_jit::JitNativeTrustedLiteralSlot;
        let constant_slots =
            view.trusted_constant_slots as usize as *mut php_jit::JitNativeTrustedConstantSlot;
        if function_offsets.is_null() || literal_slots.is_null() || constant_slots.is_null() {
            return Err("late-static callable publication tables are unavailable");
        }
        let function_base =
            usize::try_from(unsafe { *function_offsets.add(plan.function.index()) })
                .map_err(|_| "late-static function slot base does not fit usize")?;
        let mut updates = Vec::new();
        for (continuation, instruction) in instructions.iter().enumerate() {
            let Some(php_ir::Instruction {
                kind:
                    php_ir::InstructionKind::FetchClassConstant {
                        class_name,
                        constant,
                        ..
                    },
                ..
            }) = instruction.as_deref()
            else {
                continue;
            };
            if !class_name.eq_ignore_ascii_case("static") {
                continue;
            }
            let mut candidate = called_class.clone();
            let value =
                loop {
                    let class = compiled
                        .lookup_unit_class(&candidate)
                        .ok_or("late-static called class is outside the target unit")?;
                    if let Some(entry) = class
                        .constants
                        .iter()
                        .find(|entry| entry.name.eq_ignore_ascii_case(constant))
                    {
                        if entry.flags.is_private || entry.flags.is_protected {
                            return Err(
                                "late-static constant visibility requires a cold semantic boundary",
                            );
                        }
                        let literal = entry
                            .value
                            .ok_or("late-static constant is not a direct source literal")?;
                        if literal.index() >= view.trusted_literal_slot_count as usize {
                            return Err("late-static literal slot index is out of bounds");
                        }
                        let slot = unsafe { *literal_slots.add(literal.index()) };
                        if slot.state != php_jit::JIT_NATIVE_TRUSTED_LITERAL_PUBLISHED {
                            return Err("late-static literal slot is not published");
                        }
                        break slot.value;
                    }
                    candidate =
                        php_ir::module::normalize_class_name(class.parent.as_deref().ok_or(
                            "late-static constant is not declared in the class hierarchy",
                        )?);
                };
            let slot_index = function_base
                .checked_add(continuation)
                .ok_or("late-static constant slot index overflow")?;
            if slot_index >= view.trusted_constant_slot_count as usize {
                return Err("late-static constant slot index is out of bounds");
            }
            updates.push((unsafe { constant_slots.add(slot_index) }, value));
        }
        for (slot, value) in updates {
            self.retain_direct_encoded(value)?;
            let previous = unsafe { *slot };
            unsafe {
                *slot = php_jit::JitNativeTrustedConstantSlot {
                    value,
                    state: php_jit::JIT_NATIVE_TRUSTED_CONSTANT_PUBLISHED,
                    reserved: 0,
                };
            }
            if previous.state == php_jit::JIT_NATIVE_TRUSTED_CONSTANT_PUBLISHED {
                self.rollback_direct_retain(previous.value);
            }
        }
        Ok(())
    }

    /// Resolve an authoritative class-name value to one immutable published
    /// allocation plan. The returned pointer is metadata, not a PHP value.
    pub(crate) fn acquire_direct_class_plan(
        &mut self,
        class: i64,
        callback_completed: bool,
    ) -> Result<NativeClassPlanResolution, &'static str> {
        let Some(class) = self.native_string_view(class) else {
            return Ok(NativeClassPlanResolution::NotFound);
        };
        let class = Box::<[u8]>::from(class);
        let Ok(class_name) = std::str::from_utf8(&class) else {
            return Ok(NativeClassPlanResolution::NotFound);
        };
        let class_name = php_ir::module::normalize_class_name(class_name);
        let class = Box::<[u8]>::from(class_name.as_bytes());
        self.complete_direct_class_autoload_callback(&class, callback_completed)?;
        if let Some(plan) = self.internal_class_plan(&class_name) {
            self.discard_direct_class_autoload_action(&class);
            return Ok(NativeClassPlanResolution::Ready(plan));
        }
        if let Some(plan) = self
            .symbol_query
            .class_plan(&class_name, &self.header.active_runtime_view())
        {
            self.discard_direct_class_autoload_action(&class);
            return Ok(NativeClassPlanResolution::Ready(plan));
        }
        let Some(callback) = self.next_direct_class_autoload_callback(class)? else {
            return Ok(NativeClassPlanResolution::NotFound);
        };
        Ok(NativeClassPlanResolution::InvokeUserCallback(callback))
    }

    pub(crate) fn complete_direct_class_autoload_callback(
        &mut self,
        class: &[u8],
        callback_completed: bool,
    ) -> Result<(), &'static str> {
        if !callback_completed {
            return Ok(());
        }
        let Some(action) = self
            .class_autoload_actions
            .last_mut()
            .filter(|action| action.name.as_ref() == class)
        else {
            return Err("completed class autoload callback has no active action");
        };
        if !action.callback_in_flight {
            return Err("class autoload callback completion was not pending");
        }
        action.callback_in_flight = false;
        Ok(())
    }

    pub(crate) fn discard_direct_class_autoload_action(&mut self, class: &[u8]) {
        if self
            .class_autoload_actions
            .last()
            .is_some_and(|action| action.name.as_ref() == class)
        {
            self.class_autoload_actions.pop();
        }
    }

    pub(crate) fn next_direct_class_autoload_callback(
        &mut self,
        class: Box<[u8]>,
    ) -> Result<Option<i64>, &'static str> {
        let last_index = self.class_autoload_actions.len().checked_sub(1);
        if self
            .class_autoload_actions
            .iter()
            .enumerate()
            .any(|(index, action)| {
                action.name.as_ref() == class.as_ref()
                    && (action.callback_in_flight || Some(index) != last_index)
            })
        {
            return Ok(None);
        }
        if !self
            .class_autoload_actions
            .last()
            .is_some_and(|action| action.name.as_ref() == class.as_ref())
        {
            self.class_autoload_actions.push(NativeClassAutoloadAction {
                name: class,
                next_callback: 0,
                callback_in_flight: false,
            });
        }
        let Some(action) = self.class_autoload_actions.last_mut() else {
            return Err("autoload publication failed");
        };
        // SAFETY: request activation owns the separately boxed callback
        // registry for at least as long as this fast-state capability.
        #[allow(unsafe_code)]
        let callbacks = unsafe { self.callback_handlers.as_ref() };
        let Some(callbacks) = callbacks else {
            self.class_autoload_actions.pop();
            return Ok(None);
        };
        let Some(callback) = callbacks.autoload_callbacks.get(action.next_callback) else {
            self.class_autoload_actions.pop();
            return Ok(None);
        };
        action.next_callback = action.next_callback.saturating_add(1);
        action.callback_in_flight = true;
        Ok(Some(callback.callable))
    }

    /// Publishes one callable whose target and signature were fixed before
    /// optimizer entry. This allocation performs no symbol lookup or dynamic
    /// dispatch; the supplied plan is the complete callable contract.
    fn publish_fixed_function_callable(
        &mut self,
        name: &[u8],
        plan: NativeFixedCallablePlan,
    ) -> Result<i64, &'static str> {
        self.publish_prepared_callable_owned(NativePreparedCallableOwner::user_function(
            Box::from(name),
            Some(plan),
        ))
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn current_execution_scope(&self) -> Option<&NativeExecutionScope> {
        unsafe { self.execution_scope.as_ref() }
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    pub(crate) fn native_string_view(&self, encoded: i64) -> Option<&[u8]> {
        let encoded = self.native_by_value_encoding(encoded)?;
        if let Some((_, slot)) = self.direct_slot(encoded) {
            if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_STRING {
                return None;
            }
            let length = usize::try_from(slot.payload).ok()?;
            if length == 0 {
                return Some(&[]);
            }
            let bytes = slot.aux as usize as *const u8;
            if bytes.is_null() {
                return None;
            }
            // SAFETY: the direct string descriptor points into the stable
            // request-owned byte arena for its published length.
            return Some(unsafe { std::slice::from_raw_parts(bytes, length) });
        }
        let constant = php_jit::jit_decode_constant(encoded)?;
        let view = self.header.active_runtime_view();
        if constant >= view.trusted_constant_view_count {
            return None;
        }
        let constants =
            view.trusted_constant_views as usize as *const php_jit::JitNativeConstantView;
        // SAFETY: publication owns a dense descriptor array for the active
        // unit and the index was checked against its exact count.
        let constant = unsafe { *constants.add(constant as usize) };
        if constant.kind != php_jit::JIT_NATIVE_CONSTANT_VIEW_STRING {
            return None;
        }
        let length = usize::try_from(constant.length).ok()?;
        if length == 0 {
            return Some(&[]);
        }
        let bytes = constant.bytes as usize as *const u8;
        if bytes.is_null() {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(bytes, length) })
    }

    /// Returns the request-stable backing range for an authoritative native
    /// string. Exact parsers use this representation when they must mutate
    /// another native arena through `self` while the input remains borrowed.
    ///
    /// The encoded owner must remain live for the complete synchronous use of
    /// the returned range.
    pub(crate) fn stable_native_string_range(&self, encoded: i64) -> Option<(*const u8, usize)> {
        let bytes = self.native_string_view(encoded)?;
        Some((bytes.as_ptr(), bytes.len()))
    }

    /// Returns the request-stable backing range for an authoritative native
    /// array. Exact handlers use this representation when publication needs
    /// mutable access to another native arena while the input remains live.
    ///
    /// The encoded array owner must remain live for the complete synchronous
    /// use of the returned range. Native arena reservations do not relocate
    /// or overwrite a live array range.
    pub(crate) fn stable_native_array_range(
        &self,
        encoded: i64,
    ) -> Option<(*const php_jit::JitNativeDirectArrayEntry, usize)> {
        let entries = self.native_direct_array_entries(encoded)?;
        Some((entries.as_ptr(), entries.len()))
    }

    /// Follows only authoritative direct references and borrows the published
    /// object owner. Materialized compatibility objects remain baseline-only.
    fn native_query_object(&self, encoded: i64) -> Option<&php_runtime::api::ObjectRef> {
        self.direct_object(encoded)
    }

    fn native_serialize_precision(&self) -> Option<i32> {
        self.configuration
            .ini_registry()
            .get("serialize_precision")
            .and_then(|value| value.trim().parse().ok())
            .or(Some(-1))
    }

    fn write_native_serialized_key(
        &self,
        encoded: i64,
        output: &mut NativeDirectByteWriter<'_>,
    ) -> Option<()> {
        match self.native_printf_scalar(encoded)? {
            php_runtime::api::NativePrintfScalar::Int(value) => {
                output.write(b"i:")?;
                output.write_i64(value)?;
                output.write(b";")
            }
            php_runtime::api::NativePrintfScalar::String(bytes) => {
                output.write(b"s:")?;
                output.write_usize(bytes.len())?;
                output.write(b":\"")?;
                output.write(bytes)?;
                output.write(b"\";")
            }
            _ => None,
        }
    }

    /// Serializes the authoritative native scalar/array graph directly into
    /// PHP's wire format. No `Value`, copied array, or compatibility identity
    /// map is constructed on the admitted path.
    fn write_native_serialized(
        &self,
        encoded: i64,
        output: &mut NativeDirectByteWriter<'_>,
        depth: usize,
        traversal: &mut NativeSerializationTraversal,
    ) -> Option<()> {
        if depth > NativeSerializedParser::MAX_DEPTH {
            return None;
        }
        if let Some((index, slot)) = self.direct_slot(encoded) {
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                && native_reference_state(slot.reserved)
                    != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
            {
                if traversal.reference_is_active(index) {
                    return output.write(b"N;");
                }
                traversal.push_reference(index)?;
                let result =
                    self.write_native_serialized(slot.payload as i64, output, depth + 1, traversal);
                traversal.pop_reference();
                return result;
            }
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY {
                if traversal.array_is_active(index) {
                    return None;
                }
                let entries = self.native_direct_array_entries(encoded)?;
                output.write(b"a:")?;
                output.write_usize(entries.len())?;
                output.write(b":{")?;
                traversal.push_array(index)?;
                for entry in entries {
                    self.write_native_serialized_key(entry.key, output)?;
                    self.write_native_serialized(entry.value, output, depth + 1, traversal)?;
                }
                traversal.pop_array();
                return output.write_byte(b'}');
            }
            if self.native_resource_view(encoded).is_some() {
                return output.write(b"i:0;");
            }
        }
        match self.native_printf_scalar(encoded)? {
            php_runtime::api::NativePrintfScalar::Null => output.write(b"N;")?,
            php_runtime::api::NativePrintfScalar::Bool(false) => output.write(b"b:0;")?,
            php_runtime::api::NativePrintfScalar::Bool(true) => output.write(b"b:1;")?,
            php_runtime::api::NativePrintfScalar::Int(value) => {
                output.write(b"i:")?;
                output.write_i64(value)?;
                output.write(b";")?;
            }
            php_runtime::api::NativePrintfScalar::Float(value) => {
                let mut float = [0_u8; php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY];
                output.write(b"d:")?;
                output.write(php_runtime::api::float_to_php_string_bytes(
                    value, &mut float,
                ))?;
                output.write_byte(b';')?;
            }
            php_runtime::api::NativePrintfScalar::String(bytes) => {
                output.write(b"s:")?;
                output.write_usize(bytes.len())?;
                output.write(b":\"")?;
                output.write(bytes)?;
                output.write(b"\";")?;
            }
        }
        Some(())
    }

    fn write_native_serialized_root(
        &self,
        encoded: i64,
        output: &mut NativeDirectByteWriter<'_>,
    ) -> Option<()> {
        // Positive serialize_precision uses PHP's `%G` formatting. Keep that
        // uncommon request option on the exact baseline continuation until
        // the shared scalar formatter publishes the same native API.
        if self.native_serialize_precision()? >= 1 {
            return None;
        }
        self.write_native_serialized(encoded, output, 0, &mut NativeSerializationTraversal::new())
    }

    pub(crate) fn native_serialize_output_length(&self, encoded: i64) -> Option<usize> {
        let mut output = NativeDirectByteWriter::counting();
        self.write_native_serialized_root(encoded, &mut output)?;
        Some(output.length)
    }

    pub(crate) fn native_serialize_into(&self, encoded: i64, destination: &mut [u8]) -> bool {
        let mut output = NativeDirectByteWriter::writing(destination);
        self.write_native_serialized_root(encoded, &mut output)
            .is_some()
            && output.is_complete()
    }

    fn write_native_session_encoded(
        &self,
        output: &mut NativeDirectByteWriter<'_>,
    ) -> Option<bool> {
        if self.session.control().status() != php_runtime::api::PHP_SESSION_ACTIVE {
            return None;
        }
        let payload = self.native_session_payload()?;
        let entries = self.native_direct_array_entries(payload)?;
        let handler = self
            .configuration
            .ini_registry()
            .get("session.serialize_handler")
            .unwrap_or("php");
        if handler == "php_serialize" {
            self.write_native_serialized_root(payload, output)?;
            return Some(true);
        }
        if !matches!(handler, "php" | "php_binary") || self.native_serialize_precision()? >= 1 {
            return None;
        }

        let binary = handler == "php_binary";
        let mut traversal = NativeSerializationTraversal::new();
        let start = output.length;
        for entry in entries {
            let php_runtime::api::NativePrintfScalar::String(name) =
                self.native_printf_scalar(entry.key)?
            else {
                // Numeric session keys require a PHP warning; take the one
                // diagnostic baseline continuation before producing output.
                return None;
            };
            if binary {
                let length = u8::try_from(name.len()).ok()?;
                output.write_byte(length)?;
            }
            output.write(name)?;
            if !binary {
                output.write_byte(b'|')?;
            }
            self.write_native_serialized(entry.value, output, 0, &mut traversal)?;
        }
        Some(output.length > start)
    }

    fn native_session_encode_output_length(&self) -> Option<Option<usize>> {
        let mut output = NativeDirectByteWriter::counting();
        let has_output = self.write_native_session_encoded(&mut output)?;
        Some(has_output.then_some(output.length))
    }

    fn native_session_encode_into(&self, destination: &mut [u8]) -> bool {
        let mut output = NativeDirectByteWriter::writing(destination);
        self.write_native_session_encoded(&mut output) == Some(true) && output.is_complete()
    }

    fn native_unserialize(&mut self, encoded: i64) -> Option<i64> {
        let (bytes, length) = self.stable_native_string_range(encoded)?;
        // SAFETY: the encoded input remains owned for this synchronous parse,
        // and publication mutates disjoint request arenas.
        #[allow(unsafe_code)]
        let bytes = unsafe { std::slice::from_raw_parts(bytes, length) };
        NativeSerializedParser {
            bytes,
            offset: 0,
            parsed_items: 0,
        }
        .parse(self)
    }

    fn merge_native_session_array_owned(&mut self, decoded: i64) -> bool {
        let Some(current) = self.native_session_payload() else {
            let _ = self.discard_owned_direct_value(decoded);
            return false;
        };
        let Some((current_entries, current_length)) = self.stable_native_array_range(current)
        else {
            let _ = self.discard_owned_direct_value(decoded);
            return false;
        };
        let Some((decoded_entries, decoded_length)) = self.stable_native_array_range(decoded)
        else {
            let _ = self.discard_owned_direct_value(decoded);
            return false;
        };

        let entry_at = |entries: *const php_jit::JitNativeDirectArrayEntry, index: usize| {
            // SAFETY: both source owners remain live until replacement
            // publication is complete, and their arena ranges are stable.
            #[allow(unsafe_code)]
            unsafe {
                *entries.add(index)
            }
        };
        let key_matches_range = |fast: &Self,
                                 entries: *const php_jit::JitNativeDirectArrayEntry,
                                 length: usize,
                                 key: i64| {
            (0..length).any(|index| {
                fast.native_compare_array_keys(entry_at(entries, index).key, key)
                    == Some(std::cmp::Ordering::Equal)
            })
        };

        let appended = (0..decoded_length)
            .filter(|&decoded_index| {
                let key = entry_at(decoded_entries, decoded_index).key;
                !key_matches_range(self, current_entries, current_length, key)
                    && !key_matches_range(self, decoded_entries, decoded_index, key)
            })
            .count();
        let Some(output_length) = current_length.checked_add(appended) else {
            let _ = self.discard_owned_direct_value(decoded);
            return false;
        };

        let replacement =
            self.publish_owned_direct_array_with(output_length, |fast, output_index| {
                let selected = if output_index < current_length {
                    let current = entry_at(current_entries, output_index);
                    (0..decoded_length)
                        .rev()
                        .map(|index| entry_at(decoded_entries, index))
                        .find(|decoded| {
                            fast.native_compare_array_keys(current.key, decoded.key)
                                == Some(std::cmp::Ordering::Equal)
                        })
                        .unwrap_or(current)
                } else {
                    let appended_index = output_index - current_length;
                    let first = (0..decoded_length)
                        .filter(|&decoded_index| {
                            let key = entry_at(decoded_entries, decoded_index).key;
                            !key_matches_range(fast, current_entries, current_length, key)
                                && !key_matches_range(fast, decoded_entries, decoded_index, key)
                        })
                        .nth(appended_index)
                        .ok_or("native session merge lost an appended entry")?;
                    let first = entry_at(decoded_entries, first);
                    (0..decoded_length)
                        .rev()
                        .map(|index| entry_at(decoded_entries, index))
                        .find(|decoded| {
                            fast.native_compare_array_keys(first.key, decoded.key)
                                == Some(std::cmp::Ordering::Equal)
                        })
                        .unwrap_or(first)
                };
                fast.retain_direct_encoded(selected.key)?;
                if let Err(error) = fast.retain_direct_encoded(selected.value) {
                    fast.rollback_direct_retain(selected.key);
                    return Err(error);
                }
                Ok(selected)
            });
        let _ = self.discard_owned_direct_value(decoded);
        let Ok(replacement) = replacement else {
            return false;
        };
        self.replace_native_session_payload_owned(replacement)
    }

    fn native_session_serialized_name<'a>(
        input: &'a [u8],
        offset: &mut usize,
        binary: bool,
    ) -> Option<&'a [u8]> {
        if binary {
            let length = *input.get(*offset)? as usize;
            *offset = offset.checked_add(1)?;
            let end = offset.checked_add(length)?;
            let name = input.get(*offset..end)?;
            *offset = end;
            return Some(name);
        }
        let separator = input
            .get(*offset..)?
            .iter()
            .position(|byte| *byte == b'|')?;
        let end = offset.checked_add(separator)?;
        let name = input.get(*offset..end)?;
        *offset = end.checked_add(1)?;
        Some(name)
    }

    fn native_session_serialized_entry_count(input: &[u8], binary: bool) -> Option<usize> {
        let mut offset = 0_usize;
        let mut count = 0_usize;
        while offset < input.len() {
            Self::native_session_serialized_name(input, &mut offset, binary)?;
            let consumed = NativeSerializedCursor::skip_prefix(input.get(offset..)?)?;
            offset = offset.checked_add(consumed)?;
            count = count.checked_add(1)?;
        }
        Some(count)
    }

    fn native_session_decode(&mut self, encoded: i64) -> Option<()> {
        if self.session.control().status() != php_runtime::api::PHP_SESSION_ACTIVE {
            return None;
        }
        let handler = self
            .configuration
            .ini_registry()
            .get("session.serialize_handler")
            .unwrap_or("php");
        let decoded = match handler {
            "php_serialize" => self.native_unserialize(encoded),
            "php" | "php_binary" => {
                let binary = handler == "php_binary";
                let (input, input_length) = self.stable_native_string_range(encoded)?;
                // SAFETY: the encoded session payload remains owned for this
                // synchronous decode; native publication uses disjoint stable arenas.
                #[allow(unsafe_code)]
                let input = unsafe { std::slice::from_raw_parts(input, input_length) };
                let count = Self::native_session_serialized_entry_count(input, binary)?;
                self.publish_owned_direct_array_dynamic(count, |fast, writer| {
                    let mut offset = 0_usize;
                    for _ in 0..count {
                        let name = Self::native_session_serialized_name(input, &mut offset, binary)
                            .ok_or("native session name is malformed")?;
                        let key = fast.publish_direct_string_bytes(name)?;
                        let parsed = NativeSerializedParser {
                            bytes: input
                                .get(offset..)
                                .ok_or("native session value is outside its input")?,
                            offset: 0,
                            parsed_items: 0,
                        }
                        .parse_prefix(fast);
                        let Some((value, consumed)) = parsed else {
                            let _ = fast.discard_owned_direct_value(key);
                            return Err("native session value is malformed");
                        };
                        offset = match offset.checked_add(consumed) {
                            Some(offset) => offset,
                            None => {
                                let _ = fast.discard_owned_direct_value(value);
                                let _ = fast.discard_owned_direct_value(key);
                                return Err("native session offset overflow");
                            }
                        };
                        let entry = php_jit::JitNativeDirectArrayEntry { key, value };
                        if let Err(error) = writer.push_owned(entry) {
                            let _ = fast.discard_owned_direct_value(value);
                            let _ = fast.discard_owned_direct_value(key);
                            return Err(error);
                        }
                    }
                    (offset == input.len())
                        .then_some(())
                        .ok_or("native session input has trailing bytes")
                })
                .ok()
            }
            _ => None,
        };
        self.merge_native_session_array_owned(decoded?)
            .then_some(())
    }

    fn write_native_json_float(
        value: f64,
        output: &mut NativeDirectByteWriter<'_>,
        flags: i64,
    ) -> Option<()> {
        if !value.is_finite() {
            return None;
        }
        if value.fract() == 0.0
            && flags & php_runtime::api::NATIVE_JSON_PRESERVE_ZERO_FRACTION == 0
            && value >= i64::MIN as f64
            && value <= i64::MAX as f64
        {
            return output.write_i64(value as i64);
        }
        let mut buffer = ryu::Buffer::new();
        output.write(buffer.format_finite(value).as_bytes())
    }

    fn write_native_json_indent(
        output: &mut NativeDirectByteWriter<'_>,
        depth: usize,
    ) -> Option<()> {
        const SPACES: &[u8; 64] =
            b"                                                                ";
        output.write_byte(b'\n')?;
        let mut remaining = depth.checked_mul(4)?;
        while remaining != 0 {
            let length = remaining.min(SPACES.len());
            output.write(&SPACES[..length])?;
            remaining -= length;
        }
        Some(())
    }

    fn write_native_json_string(
        bytes: &[u8],
        output: &mut NativeDirectByteWriter<'_>,
        flags: i64,
    ) -> Option<()> {
        if flags & php_runtime::api::NATIVE_JSON_NUMERIC_CHECK != 0 {
            use php_runtime::experimental::numeric_string::{
                NumericStringKind, NumericStringValue,
            };

            let classified = php_runtime::experimental::numeric_string::classify(bytes);
            match (classified.kind, classified.value) {
                (NumericStringKind::IntString, Some(NumericStringValue::Int(value))) => {
                    return output.write_i64(value);
                }
                (NumericStringKind::FloatString, Some(NumericStringValue::Float(value)))
                    if value.is_finite() =>
                {
                    return Self::write_native_json_float(value, output, flags);
                }
                _ => {}
            }
        }
        php_runtime::api::visit_json_string_with_flags(bytes, flags, |encoded| {
            output
                .write(encoded)
                .ok_or("native JSON string output overflow")
        })
        .ok()
    }

    /// Encodes the authoritative native scalar/array/string graph using
    /// PHP's representation-preserving `json_encode` rules. Unsupported
    /// semantic shapes return before publication so the call can take its
    /// one baseline continuation without synchronizing a second value
    /// representation.
    // Architecture: exact JSON recursion carries its bounded depth and cycle
    // state explicitly and never packages it into a generic runtime context.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn write_native_json(
        &self,
        mut encoded: i64,
        output: &mut NativeDirectByteWriter<'_>,
        depth: usize,
        maximum_depth: usize,
        flags: i64,
        traversal: &mut NativeJsonTraversal,
    ) -> Option<()> {
        for _ in 0..64 {
            if let Some((index, slot)) = self.direct_slot(encoded) {
                match slot.kind {
                    php_jit::JIT_NATIVE_VALUE_VIEW_STRING => {
                        return Self::write_native_json_string(
                            self.native_string_view(encoded)?,
                            output,
                            flags,
                        );
                    }
                    php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
                        if slot.flags == php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION =>
                    {
                        return output.write_i64(slot.payload as i64);
                    }
                    php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => {
                        return Self::write_native_json_float(
                            f64::from_bits(slot.payload),
                            output,
                            flags,
                        );
                    }
                    php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                        if native_reference_state(slot.reserved)
                            != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY =>
                    {
                        encoded = slot.payload as i64;
                        continue;
                    }
                    php_jit::JIT_NATIVE_VALUE_VIEW_ARRAY
                    | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY => {
                        if depth >= maximum_depth || traversal.array_is_active(index) {
                            return None;
                        }
                        let length = usize::try_from(slot.payload).ok()?;
                        let entries = if length == 0 {
                            &[]
                        } else {
                            let entries =
                                slot.aux as usize as *const php_jit::JitNativeDirectArrayEntry;
                            if entries.is_null() {
                                return None;
                            }
                            // SAFETY: the direct-array descriptor owns this
                            // stable insertion-ordered range for the slot's
                            // live nonzero length.
                            unsafe { std::slice::from_raw_parts(entries, length) }
                        };
                        let packed = entries.iter().enumerate().all(|(position, entry)| {
                            matches!(
                                self.native_comparison_value(entry.key),
                                Some(NativeComparisonValue::Int(key))
                                    if key == i64::try_from(position).unwrap_or(i64::MAX)
                            )
                        });
                        let array_syntax =
                            packed && flags & php_runtime::api::NATIVE_JSON_FORCE_OBJECT == 0;
                        let pretty = flags & php_runtime::api::NATIVE_JSON_PRETTY_PRINT != 0;
                        traversal.push_array(index)?;
                        output.write_byte(if array_syntax { b'[' } else { b'{' })?;
                        for (position, entry) in entries.iter().enumerate() {
                            if position == 0 {
                                if pretty {
                                    Self::write_native_json_indent(output, depth + 1)?;
                                }
                            } else {
                                output.write_byte(b',')?;
                                if pretty {
                                    Self::write_native_json_indent(output, depth + 1)?;
                                }
                            }
                            if !array_syntax {
                                match self.native_comparison_value(entry.key)? {
                                    NativeComparisonValue::Int(key) => {
                                        output.write_byte(b'"')?;
                                        output.write_i64(key)?;
                                        output.write_byte(b'"')?;
                                    }
                                    NativeComparisonValue::String(key) => {
                                        Self::write_native_json_string(key, output, flags)?;
                                    }
                                    _ => return None,
                                }
                                output.write_byte(b':')?;
                                if pretty {
                                    output.write_byte(b' ')?;
                                }
                            }
                            self.write_native_json(
                                entry.value,
                                output,
                                depth + 1,
                                maximum_depth,
                                flags,
                                traversal,
                            )?;
                        }
                        if pretty && !entries.is_empty() {
                            Self::write_native_json_indent(output, depth)?;
                        }
                        output.write_byte(if array_syntax { b']' } else { b'}' })?;
                        traversal.pop_array();
                        return Some(());
                    }
                    php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
                        if php_jit::jit_native_object_property_view_is_published(slot.flags) =>
                    {
                        let owner = self.direct_object(encoded)?;
                        let identity = owner.id();
                        if depth >= maximum_depth
                            || traversal.object_is_active(identity)
                            || owner.is_enum()
                            || owner
                                .class_name_handle()
                                .eq_ignore_ascii_case("splfixedarray")
                        {
                            return None;
                        }
                        let pretty = flags & php_runtime::api::NATIVE_JSON_PRETTY_PRINT != 0;
                        traversal.push_object(identity)?;
                        let result = owner
                            .with_native_array_cast_view(
                                slot.payload,
                                |declared_names, declared, dynamic_order, dynamic| {
                                    output.write_byte(b'{')?;
                                    let mut position = 0_usize;
                                    let mut append_property =
                                        |name: &[u8], value: i64| -> Option<()> {
                                            if position == 0 {
                                                if pretty {
                                                    Self::write_native_json_indent(
                                                        output,
                                                        depth + 1,
                                                    )?;
                                                }
                                            } else {
                                                output.write_byte(b',')?;
                                                if pretty {
                                                    Self::write_native_json_indent(
                                                        output,
                                                        depth + 1,
                                                    )?;
                                                }
                                            }
                                            Self::write_native_json_string(name, output, flags)?;
                                            output.write_byte(b':')?;
                                            if pretty {
                                                output.write_byte(b' ')?;
                                            }
                                            self.write_native_json(
                                                value,
                                                output,
                                                depth + 1,
                                                maximum_depth,
                                                flags,
                                                traversal,
                                            )?;
                                            position += 1;
                                            Some(())
                                        };
                                    for (name, property) in declared_names.iter().zip(declared) {
                                        if property.initialized == 0 || name.starts_with('\0') {
                                            continue;
                                        }
                                        append_property(name.as_bytes(), property.value)?;
                                    }
                                    for name in dynamic_order {
                                        let property = dynamic.get(name)?;
                                        if property.slot.initialized == 0 {
                                            continue;
                                        }
                                        append_property(name.as_bytes(), property.slot.value)?;
                                    }
                                    if pretty && position != 0 {
                                        Self::write_native_json_indent(output, depth)?;
                                    }
                                    output.write_byte(b'}')?;
                                    Some(())
                                },
                            )
                            .flatten();
                        traversal.pop_object();
                        return result;
                    }
                    // Baseline references, special object families, and
                    // extension values retain their exact cold continuation.
                    _ => return None,
                }
            }
            if php_jit::jit_decode_runtime_value(encoded).is_none()
                && php_jit::jit_decode_constant(encoded).is_none()
            {
                return output.write_i64(encoded);
            }
            let constant = php_jit::jit_decode_constant(encoded)?;
            match constant {
                u32::MAX | php_jit::JIT_VALUE_UNINITIALIZED => output.write(b"null")?,
                php_jit::JIT_VALUE_FALSE => output.write(b"false")?,
                php_jit::JIT_VALUE_TRUE => output.write(b"true")?,
                _ => {
                    let view = self.header.active_runtime_view();
                    if constant >= view.trusted_constant_view_count {
                        return None;
                    }
                    let constants = view.trusted_constant_views as usize
                        as *const php_jit::JitNativeConstantView;
                    let constant = unsafe { *constants.add(constant as usize) };
                    match constant.kind {
                        php_jit::JIT_NATIVE_CONSTANT_VIEW_NULL => output.write(b"null")?,
                        php_jit::JIT_NATIVE_CONSTANT_VIEW_BOOL => {
                            output.write(if constant.length != 0 {
                                b"true"
                            } else {
                                b"false"
                            })?
                        }
                        php_jit::JIT_NATIVE_CONSTANT_VIEW_INT => {
                            output.write_i64(constant.length as i64)?;
                        }
                        php_jit::JIT_NATIVE_CONSTANT_VIEW_FLOAT => {
                            Self::write_native_json_float(
                                f64::from_bits(constant.length),
                                output,
                                flags,
                            )?;
                        }
                        php_jit::JIT_NATIVE_CONSTANT_VIEW_STRING => {
                            Self::write_native_json_string(
                                self.native_string_view(encoded)?,
                                output,
                                flags,
                            )?;
                        }
                        _ => return None,
                    }
                }
            }
            return Some(());
        }
        None
    }

    fn native_json_output_length(
        &self,
        encoded: i64,
        maximum_depth: usize,
        flags: i64,
    ) -> Option<usize> {
        let mut output = NativeDirectByteWriter::counting();
        self.write_native_json(
            encoded,
            &mut output,
            0,
            maximum_depth,
            flags,
            &mut NativeJsonTraversal::new(),
        )?;
        Some(output.length)
    }

    fn native_json_into(
        &self,
        encoded: i64,
        maximum_depth: usize,
        flags: i64,
        destination: &mut [u8],
    ) -> bool {
        let mut output = NativeDirectByteWriter::writing(destination);
        self.write_native_json(
            encoded,
            &mut output,
            0,
            maximum_depth,
            flags,
            &mut NativeJsonTraversal::new(),
        )
        .is_some()
            && output.is_complete()
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn validate_native_json(
        &self,
        input: i64,
        depth: i64,
        flags: i64,
    ) -> Option<Result<bool, php_runtime::api::BuiltinError>> {
        let state = self.json_state;
        let input = self.native_string_view(input)?;
        let state = unsafe { state.as_mut() }?;
        Some(php_runtime::api::validate_native_json(
            state, input, depth, flags,
        ))
    }

    /// Writes retained entries directly into the authoritative array arena.
    ///
    /// Retains are transactional: if any owner or arena reservation fails,
    /// the already-retained owners are rolled back from the target range.
    /// No mirror `Vec` or retain ledger is constructed.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn publish_retained_direct_array_from_iter(
        &mut self,
        entries: impl ExactSizeIterator<Item = php_jit::JitNativeDirectArrayEntry>,
    ) -> Result<i64, &'static str> {
        let view = self.header.active_runtime_view();
        let length = entries.len();
        let (start, capacity) = self.reserve_direct_array_range(length)?;
        let storage = view.direct_array_entries as usize as *mut php_jit::JitNativeDirectArrayEntry;
        let mut next_append_key: Option<i64> = None;
        let mut retained = 0_usize;
        for (offset, entry) in entries.enumerate() {
            if let Some(php_runtime::api::NativePrintfScalar::Int(key)) =
                self.native_printf_scalar(entry.key)
            {
                next_append_key = Some(next_append_key.map_or(key.saturating_add(1), |current| {
                    current.max(key.saturating_add(1))
                }));
            }
            unsafe {
                *storage.add(start + offset) = entry;
            }
            for encoded in [entry.key, entry.value] {
                if let Err(error) = self.retain_direct_encoded(encoded) {
                    for rollback in (0..retained).rev() {
                        let entry = unsafe { *storage.add(start + rollback / 2) };
                        self.rollback_direct_retain(if rollback % 2 == 0 {
                            entry.key
                        } else {
                            entry.value
                        });
                    }
                    self.free_direct_array_range(start, capacity);
                    return Err(error);
                }
                retained += 1;
            }
        }
        let index = match self.reserve_direct_value_index() {
            Ok(index) => index,
            Err(error) => {
                for rollback in (0..retained).rev() {
                    let entry = unsafe { *storage.add(start + rollback / 2) };
                    self.rollback_direct_retain(if rollback % 2 == 0 {
                        entry.key
                    } else {
                        entry.value
                    });
                }
                self.free_direct_array_range(start, capacity);
                return Err(error);
            }
        };
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        let states = view.direct_array_states as usize as *mut php_jit::JitNativeDirectArrayState;
        unsafe {
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                refcount: 1,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
                flags: php_jit::jit_native_direct_array_flags(None),
                reserved: capacity,
                payload: length as u64,
                aux: storage.add(start) as usize as u64,
            };
            *states.add(index as usize) = php_jit::JitNativeDirectArrayState {
                next_append_key: next_append_key.unwrap_or(0),
                has_next_append_key: u32::from(next_append_key.is_some()),
                reserved: 0,
            };
        }
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
        Ok((php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG | u64::from(runtime_index)) as i64)
    }

    /// Reserves one unpublished authoritative array range and commits only the
    /// prefix written by `build`. This is the direct boundary for parsers and
    /// recursive producers whose final entry count can be smaller than their
    /// PHP-visible maximum because duplicate keys replace earlier values.
    ///
    /// Failed construction or final value-slot publication releases every
    /// transferred owner and returns the range to the native arena.
    #[allow(unsafe_code)] // Safety: the native request owns every unpublished range for the synchronous activation.
    fn begin_owned_direct_array(
        &mut self,
        initial_capacity: usize,
        maximum_length: usize,
    ) -> Result<NativeOwnedDirectArrayWriter, &'static str> {
        let view = self.header.active_runtime_view();
        let (start, capacity) = self.reserve_direct_array_range(initial_capacity)?;
        let storage = view.direct_array_entries as usize as *mut php_jit::JitNativeDirectArrayEntry;
        Ok(NativeOwnedDirectArrayWriter {
            entries: unsafe { storage.add(start) },
            start,
            capacity,
            length: 0,
            maximum_length,
        })
    }

    #[allow(unsafe_code)] // Safety: initialized entries exclusively belong to this unpublished writer.
    fn abort_owned_direct_array(&mut self, writer: NativeOwnedDirectArrayWriter) {
        for rollback in (0..writer.length).rev() {
            let entry = unsafe { *writer.entries.add(rollback) };
            let _ = self.discard_owned_direct_value(entry.value);
            let _ = self.discard_owned_direct_value(entry.key);
        }
        self.free_direct_array_range(writer.start, writer.capacity);
    }

    #[allow(unsafe_code)] // Safety: both ranges are unpublished and exclusively owned by this request.
    fn grow_owned_direct_array(
        &mut self,
        writer: &mut NativeOwnedDirectArrayWriter,
    ) -> Result<(), &'static str> {
        if writer.length < writer.capacity as usize {
            return Ok(());
        }
        if writer.length >= writer.maximum_length {
            return Err("native direct array writer exceeded its maximum length");
        }
        let requested = (writer.capacity as usize)
            .checked_mul(2)
            .ok_or("native direct array writer capacity overflow")?
            .min(writer.maximum_length);
        let view = self.header.active_runtime_view();
        let storage = view.direct_array_entries as usize as *mut php_jit::JitNativeDirectArrayEntry;
        let (next_start, next_capacity) = self.reserve_direct_array_range(requested)?;
        let next_entries = unsafe { storage.add(next_start) };
        unsafe {
            std::ptr::copy_nonoverlapping(writer.entries, next_entries, writer.length);
        }
        self.free_direct_array_range(writer.start, writer.capacity);
        writer.entries = next_entries;
        writer.start = next_start;
        writer.capacity = next_capacity;
        Ok(())
    }

    fn push_owned_direct_array_entry(
        &mut self,
        writer: &mut NativeOwnedDirectArrayWriter,
        entry: php_jit::JitNativeDirectArrayEntry,
    ) -> Result<(), &'static str> {
        self.grow_owned_direct_array(writer)?;
        writer.push_owned(entry)
    }

    /// Mutates one freshly published, uniquely owned direct array in place.
    /// Query parsing uses published child handles as its native tree links;
    /// the array contents themselves remain authoritative arena storage.
    #[allow(unsafe_code)] // Safety: the array is uniquely owned by the unpublished parse result.
    fn mutate_owned_direct_array<R>(
        &mut self,
        array: i64,
        mutate: impl FnOnce(&mut Self, &mut NativeOwnedDirectArrayWriter) -> Result<R, &'static str>,
    ) -> Result<R, &'static str> {
        let (index, slot) = self
            .direct_slot(array)
            .ok_or("native input array owner is unavailable")?;
        if slot.refcount != 1 || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY {
            return Err("native input array is not uniquely owned direct storage");
        }
        let view = self.header.active_runtime_view();
        let storage = view.direct_array_entries as usize as *mut php_jit::JitNativeDirectArrayEntry;
        let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
        let offset = (slot.aux as usize)
            .checked_sub(storage as usize)
            .ok_or("native input array is outside its stable arena")?;
        if !offset.is_multiple_of(entry_size) {
            return Err("native input array is not entry-aligned");
        }
        let start = offset / entry_size;
        let length =
            usize::try_from(slot.payload).map_err(|_| "native input array length overflow")?;
        if slot.reserved == 0 || !slot.reserved.is_power_of_two() || length > slot.reserved as usize
        {
            return Err("native input array capacity is invalid");
        }
        let mut writer = NativeOwnedDirectArrayWriter {
            entries: unsafe { storage.add(start) },
            start,
            capacity: slot.reserved,
            length,
            maximum_length: php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY,
        };
        let result = mutate(self, &mut writer);

        let mut next_append_key: Option<i64> = None;
        for offset in 0..writer.length {
            let entry = unsafe { *writer.entries.add(offset) };
            if let Some(php_runtime::api::NativePrintfScalar::Int(key)) =
                self.native_printf_scalar(entry.key)
            {
                next_append_key = Some(next_append_key.map_or(key.saturating_add(1), |current| {
                    current.max(key.saturating_add(1))
                }));
            }
        }
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        let states = view.direct_array_states as usize as *mut php_jit::JitNativeDirectArrayState;
        unsafe {
            (*slots.add(index)).flags = php_jit::jit_native_direct_array_flags(None);
            (*slots.add(index)).reserved = writer.capacity;
            (*slots.add(index)).payload = writer.length as u64;
            (*slots.add(index)).aux = writer.entries as usize as u64;
            *states.add(index) = php_jit::JitNativeDirectArrayState {
                next_append_key: next_append_key.unwrap_or(0),
                has_next_append_key: u32::from(next_append_key.is_some()),
                reserved: 0,
            };
        }
        result
    }

    /// Reclaims unused tail capacity from one uniquely owned direct array
    /// after an in-place native producer has shortened its initialized prefix.
    #[allow(unsafe_code)] // Safety: the unique owner keeps both arena ranges unpublished while the prefix moves.
    fn shrink_owned_direct_array_to_fit(&mut self, array: i64) -> Result<(), &'static str> {
        let (index, slot) = self
            .direct_slot(array)
            .ok_or("native result array owner is unavailable")?;
        if slot.refcount != 1 || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY {
            return Err("native result array is not uniquely owned direct storage");
        }
        let view = self.header.active_runtime_view();
        let storage = view.direct_array_entries as usize as *mut php_jit::JitNativeDirectArrayEntry;
        let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
        let offset = (slot.aux as usize)
            .checked_sub(storage as usize)
            .ok_or("native result array is outside its stable arena")?;
        if !offset.is_multiple_of(entry_size) {
            return Err("native result array is not entry-aligned");
        }
        let start = offset / entry_size;
        let length =
            usize::try_from(slot.payload).map_err(|_| "native result array length overflow")?;
        let capacity = slot.reserved;
        if capacity == 0 || !capacity.is_power_of_two() || length > capacity as usize {
            return Err("native result array capacity is invalid");
        }
        let (next_start, next_capacity) = self.reserve_direct_array_range(length)?;
        if next_capacity >= capacity {
            self.free_direct_array_range(next_start, next_capacity);
            return Ok(());
        }
        let next_entries = unsafe { storage.add(next_start) };
        unsafe {
            std::ptr::copy_nonoverlapping(storage.add(start), next_entries, length);
        }
        self.free_direct_array_range(start, capacity);
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        unsafe {
            (*slots.add(index)).reserved = next_capacity;
            (*slots.add(index)).aux = next_entries as usize as u64;
        }
        Ok(())
    }

    #[allow(unsafe_code)] // Safety: the initialized prefix exclusively belongs to this unpublished writer.
    fn finish_owned_direct_array(
        &mut self,
        writer: NativeOwnedDirectArrayWriter,
    ) -> Result<i64, &'static str> {
        let mut next_append_key: Option<i64> = None;
        for offset in 0..writer.length {
            let entry = unsafe { *writer.entries.add(offset) };
            if let Some(php_runtime::api::NativePrintfScalar::Int(key)) =
                self.native_printf_scalar(entry.key)
            {
                next_append_key = Some(next_append_key.map_or(key.saturating_add(1), |current| {
                    current.max(key.saturating_add(1))
                }));
            }
        }
        let index = match self.reserve_direct_value_index() {
            Ok(index) => index,
            Err(error) => {
                self.abort_owned_direct_array(writer);
                return Err(error);
            }
        };
        let view = self.header.active_runtime_view();
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        let states = view.direct_array_states as usize as *mut php_jit::JitNativeDirectArrayState;
        unsafe {
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                refcount: 1,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
                flags: php_jit::jit_native_direct_array_flags(None),
                reserved: writer.capacity,
                payload: writer.length as u64,
                aux: writer.entries as usize as u64,
            };
            *states.add(index as usize) = php_jit::JitNativeDirectArrayState {
                next_append_key: next_append_key.unwrap_or(0),
                has_next_append_key: u32::from(next_append_key.is_some()),
                reserved: 0,
            };
        }
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
        Ok((php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG | u64::from(runtime_index)) as i64)
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn publish_owned_direct_array_dynamic(
        &mut self,
        maximum_length: usize,
        build: impl FnOnce(&mut Self, &mut NativeOwnedDirectArrayWriter) -> Result<(), &'static str>,
    ) -> Result<i64, &'static str> {
        let mut writer = self.begin_owned_direct_array(maximum_length, maximum_length)?;
        if let Err(error) = build(self, &mut writer) {
            self.abort_owned_direct_array(writer);
            return Err(error);
        }
        self.finish_owned_direct_array(writer)
    }

    /// Transfers already-owned entries directly into the authoritative array
    /// arena. Failed publication releases those owners from the target range
    /// without cloning the complete entry vector.
    ///
    /// This builder form is for producers that create each owned key/value
    /// only after the arena reservation succeeds. A failed item build rolls
    /// back every previously completed entry in reverse ownership order.
    pub(crate) fn publish_owned_direct_array_with(
        &mut self,
        length: usize,
        mut build: impl FnMut(
            &mut Self,
            usize,
        ) -> Result<php_jit::JitNativeDirectArrayEntry, &'static str>,
    ) -> Result<i64, &'static str> {
        self.publish_owned_direct_array_dynamic(length, |fast, writer| {
            for index in 0..length {
                let entry = build(fast, index)?;
                if let Err(error) = writer.push_owned(entry) {
                    let _ = fast.discard_owned_direct_value(entry.value);
                    let _ = fast.discard_owned_direct_value(entry.key);
                    return Err(error);
                }
            }
            Ok(())
        })
    }

    /// Transfers an existing exact-size sequence of already-owned entries
    /// into the authoritative array arena.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn publish_owned_direct_array_from_iter(
        &mut self,
        mut entries: impl ExactSizeIterator<Item = php_jit::JitNativeDirectArrayEntry>,
    ) -> Result<i64, &'static str> {
        let view = self.header.active_runtime_view();
        let length = entries.len();
        let (start, capacity) = match self.reserve_direct_array_range(length) {
            Ok(range) => range,
            Err(error) => {
                for entry in entries {
                    let _ = self.discard_owned_direct_value(entry.value);
                    let _ = self.discard_owned_direct_value(entry.key);
                }
                return Err(error);
            }
        };
        let storage = view.direct_array_entries as usize as *mut php_jit::JitNativeDirectArrayEntry;
        let mut next_append_key: Option<i64> = None;
        for (offset, entry) in entries.by_ref().enumerate() {
            if let Some(php_runtime::api::NativePrintfScalar::Int(key)) =
                self.native_printf_scalar(entry.key)
            {
                next_append_key = Some(next_append_key.map_or(key.saturating_add(1), |current| {
                    current.max(key.saturating_add(1))
                }));
            }
            unsafe {
                *storage.add(start + offset) = entry;
            }
        }
        let index = match self.reserve_direct_value_index() {
            Ok(index) => index,
            Err(error) => {
                for offset in (0..length).rev() {
                    let entry = unsafe { *storage.add(start + offset) };
                    let _ = self.discard_owned_direct_value(entry.value);
                    let _ = self.discard_owned_direct_value(entry.key);
                }
                self.free_direct_array_range(start, capacity);
                return Err(error);
            }
        };
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        let states = view.direct_array_states as usize as *mut php_jit::JitNativeDirectArrayState;
        unsafe {
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                refcount: 1,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
                flags: php_jit::jit_native_direct_array_flags(None),
                reserved: capacity,
                payload: length as u64,
                aux: storage.add(start) as usize as u64,
            };
            *states.add(index as usize) = php_jit::JitNativeDirectArrayState {
                next_append_key: next_append_key.unwrap_or(0),
                has_next_append_key: u32::from(next_append_key.is_some()),
                reserved: 0,
            };
        }
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
        Ok((php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG | u64::from(runtime_index)) as i64)
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_json_last_error(&self) -> Option<(i64, &str)> {
        let state = unsafe { self.json_state.as_ref() }?;
        Some(state.value())
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_pcre_last_error(&self) -> Option<(i64, &str)> {
        let state = unsafe { self.pcre_state.as_ref() }?.last_error();
        Some((state.code(), state.message()))
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_preg_match_direct(
        &mut self,
        pattern: i64,
        subject: i64,
        flags: i64,
        offset: i64,
        publish_captures: bool,
    ) -> Option<
        Result<
            Option<php_runtime::api::NativePregPublishedMatch<i64>>,
            php_runtime::api::BuiltinError,
        >,
    > {
        let limits = self.native_pcre_limits()?;
        let state = self.pcre_state;
        let (pattern, pattern_length) = self.stable_native_string_range(pattern)?;
        let (subject, subject_length) = self.stable_native_string_range(subject)?;
        let pattern = unsafe { std::slice::from_raw_parts(pattern, pattern_length) };
        let subject = unsafe { std::slice::from_raw_parts(subject, subject_length) };
        let state = unsafe { state.as_mut() }?;
        Some(php_runtime::api::native_preg_match_into(
            state,
            limits,
            pattern,
            subject,
            flags,
            offset,
            publish_captures,
            self,
        ))
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_preg_match_all_direct(
        &mut self,
        pattern: i64,
        subject: i64,
        flags: i64,
        offset: i64,
        publish_captures: bool,
    ) -> Option<
        Result<
            Option<php_runtime::api::NativePregPublishedMatchAll<i64>>,
            php_runtime::api::BuiltinError,
        >,
    > {
        let limits = self.native_pcre_limits()?;
        let state = self.pcre_state;
        let (pattern, pattern_length) = self.stable_native_string_range(pattern)?;
        let (subject, subject_length) = self.stable_native_string_range(subject)?;
        let pattern = unsafe { std::slice::from_raw_parts(pattern, pattern_length) };
        let subject = unsafe { std::slice::from_raw_parts(subject, subject_length) };
        let state = unsafe { state.as_mut() }?;
        Some(php_runtime::api::native_preg_match_all_into(
            state,
            limits,
            pattern,
            subject,
            flags,
            offset,
            publish_captures,
            self,
        ))
    }

    #[allow(unsafe_code)] // Safety: the request owns every stable native source and publication arena.
    fn native_preg_callback_plan_direct(
        &mut self,
        pattern: i64,
        subject: i64,
        limit: i64,
        flags: i64,
    ) -> php_runtime::api::NativePregCallbackPlanResult<i64> {
        let Some(limits) = self.native_pcre_limits() else {
            return php_runtime::api::NativePregCallbackPlanResult::Unsupported;
        };
        let state = self.pcre_state;
        let Some((pattern, pattern_length)) = self.stable_native_string_range(pattern) else {
            return php_runtime::api::NativePregCallbackPlanResult::Unsupported;
        };
        let Some((subject, subject_length)) = self.stable_native_string_range(subject) else {
            return php_runtime::api::NativePregCallbackPlanResult::Unsupported;
        };
        let pattern = unsafe { std::slice::from_raw_parts(pattern, pattern_length) };
        let subject = unsafe { std::slice::from_raw_parts(subject, subject_length) };
        let Some(state) = (unsafe { state.as_mut() }) else {
            return php_runtime::api::NativePregCallbackPlanResult::Unsupported;
        };
        php_runtime::api::native_preg_callback_plan_into(
            state, limits, pattern, subject, limit, flags, self,
        )
    }

    /// Assembles callback replacements from the immutable native match plan.
    ///
    /// Generated code invokes the prepared PHP callback and publishes one
    /// native string per match. This fixed boundary only joins those strings
    /// with untouched subject spans; it never dispatches a callback or
    /// materializes a Rust `Value`.
    #[allow(unsafe_code)] // Safety: all three owners keep their request-published ranges stable.
    fn native_preg_callback_assemble_direct(
        &mut self,
        subject: i64,
        plan: i64,
        replacements: i64,
    ) -> Result<i64, &'static str> {
        #[allow(unsafe_code)]
        unsafe fn direct_slot(
            view: php_jit::JitNativeRuntimeView,
            encoded: i64,
        ) -> Option<php_jit::JitNativeValueSlot> {
            let runtime_index = php_jit::jit_decode_runtime_value(encoded)?;
            let index =
                runtime_index.checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)? as usize;
            if index >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
                return None;
            }
            let slots = view.direct_value_slots as usize as *const php_jit::JitNativeValueSlot;
            let slot = unsafe { *slots.add(index) };
            (slot.refcount != 0).then_some(slot)
        }

        #[allow(unsafe_code)]
        unsafe fn string_range(
            view: php_jit::JitNativeRuntimeView,
            encoded: i64,
        ) -> Option<(*const u8, usize)> {
            if let Some(slot) = unsafe { direct_slot(view, encoded) } {
                if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_STRING {
                    return None;
                }
                return Some((
                    slot.aux as usize as *const u8,
                    usize::try_from(slot.payload).ok()?,
                ));
            }
            let constant = php_jit::jit_decode_constant(encoded)?;
            if constant >= view.trusted_constant_view_count {
                return None;
            }
            let constants =
                view.trusted_constant_views as usize as *const php_jit::JitNativeConstantView;
            let constant = unsafe { *constants.add(constant as usize) };
            (constant.kind == php_jit::JIT_NATIVE_CONSTANT_VIEW_STRING).then_some((
                constant.bytes as usize as *const u8,
                usize::try_from(constant.length).ok()?,
            ))
        }

        #[allow(unsafe_code)]
        unsafe fn array_range(
            view: php_jit::JitNativeRuntimeView,
            encoded: i64,
        ) -> Option<(*const php_jit::JitNativeDirectArrayEntry, usize)> {
            let slot = unsafe { direct_slot(view, encoded) }?;
            (slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY).then_some((
                slot.aux as usize as *const php_jit::JitNativeDirectArrayEntry,
                usize::try_from(slot.payload).ok()?,
            ))
        }

        let view = self.header.active_runtime_view();
        let (subject, subject_length) = self
            .stable_native_string_range(subject)
            .ok_or("callback replacement subject is not a native string")?;
        let (plan, plan_length) = self
            .stable_native_array_range(plan)
            .ok_or("callback replacement plan is not a native array")?;
        let (replacements, replacement_count) = self
            .stable_native_array_range(replacements)
            .ok_or("callback replacements are not a native array")?;
        if replacement_count != plan_length {
            return Err("callback replacement count does not match the native plan");
        }

        let mut output_length = subject_length;
        let mut previous_end = 0usize;
        for index in 0..plan_length {
            let row = unsafe { *plan.add(index) };
            let (row, row_length) = self
                .stable_native_array_range(row.value)
                .ok_or("callback replacement row is not a native array")?;
            if row_length != 3 {
                return Err("callback replacement row has an invalid shape");
            }
            let start = match self.native_printf_scalar(unsafe { (*row).value }) {
                Some(php_runtime::api::NativePrintfScalar::Int(value)) => {
                    usize::try_from(value).ok()
                }
                _ => None,
            }
            .ok_or("callback replacement start is not a native integer")?;
            let end = match self.native_printf_scalar(unsafe { (*row.add(1)).value }) {
                Some(php_runtime::api::NativePrintfScalar::Int(value)) => {
                    usize::try_from(value).ok()
                }
                _ => None,
            }
            .ok_or("callback replacement end is not a native integer")?;
            if start < previous_end || end < start || end > subject_length {
                return Err("callback replacement span is outside the subject");
            }
            let replacement = unsafe { *replacements.add(index) };
            let (_, replacement_length) = self
                .stable_native_string_range(replacement.value)
                .ok_or("callback replacement result is not a native string")?;
            output_length = output_length
                .checked_sub(end - start)
                .and_then(|length| length.checked_add(replacement_length))
                .ok_or("callback replacement output length overflowed")?;
            previous_end = end;
        }

        let subject = subject as usize;
        self.try_publish_direct_string_with(output_length, |output| {
            let subject =
                unsafe { std::slice::from_raw_parts(subject as *const u8, subject_length) };
            let mut source_offset = 0usize;
            let mut output_offset = 0usize;
            for index in 0..plan_length {
                let row = unsafe { *plan.add(index) };
                let (row, row_length) = unsafe { array_range(view, row.value) }
                    .ok_or("callback replacement row disappeared during assembly")?;
                if row_length != 3 {
                    return Err("callback replacement row changed during assembly");
                }
                let start = usize::try_from(unsafe { (*row).value })
                    .map_err(|_| "callback replacement start changed during assembly")?;
                let end = usize::try_from(unsafe { (*row.add(1)).value })
                    .map_err(|_| "callback replacement end changed during assembly")?;
                let replacement = unsafe { *replacements.add(index) };
                let (replacement, replacement_length) =
                    unsafe { string_range(view, replacement.value) }
                        .ok_or("callback replacement string disappeared during assembly")?;
                let prefix_length = start - source_offset;
                output[output_offset..output_offset + prefix_length]
                    .copy_from_slice(&subject[source_offset..start]);
                output_offset += prefix_length;
                let replacement =
                    unsafe { std::slice::from_raw_parts(replacement, replacement_length) };
                output[output_offset..output_offset + replacement_length]
                    .copy_from_slice(replacement);
                output_offset += replacement_length;
                source_offset = end;
            }
            output[output_offset..].copy_from_slice(&subject[source_offset..]);
            Ok(())
        })
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_preg_replace_scalar(
        &self,
        pattern: i64,
        replacement: i64,
        subject: i64,
        limit: i64,
        filter: bool,
    ) -> Option<php_runtime::api::NativePregReplaceResult> {
        let limits = self.native_pcre_limits()?;
        let state = self.pcre_state;
        let pattern = self.native_string_view(pattern)?;
        let replacement = self.native_string_view(replacement)?;
        let subject = self.native_string_view(subject)?;
        let state = unsafe { state.as_mut() }?;
        php_runtime::api::native_preg_replace_scalar(
            state,
            limits,
            pattern,
            replacement,
            subject,
            limit,
            filter,
        )
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_preg_split_direct(
        &mut self,
        pattern: i64,
        subject: i64,
        limit: i64,
        flags: i64,
    ) -> Option<i64> {
        let limits = self.native_pcre_limits()?;
        let state = self.pcre_state;
        let (pattern, pattern_length) = self.stable_native_string_range(pattern)?;
        let (subject, subject_length) = self.stable_native_string_range(subject)?;
        let pattern = unsafe { std::slice::from_raw_parts(pattern, pattern_length) };
        let subject = unsafe { std::slice::from_raw_parts(subject, subject_length) };
        let state = unsafe { state.as_mut() }?;
        php_runtime::api::native_preg_split_into(
            state, limits, pattern, subject, limit, flags, self,
        )
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    pub(crate) fn native_direct_array_entries(
        &self,
        encoded: i64,
    ) -> Option<&[php_jit::JitNativeDirectArrayEntry]> {
        let encoded = self.native_by_value_encoding(encoded)?;
        let (_, slot) = self.direct_slot(encoded)?;
        if !matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_ARRAY | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
        ) {
            return None;
        }
        let length = usize::try_from(slot.payload).ok()?;
        if length == 0 {
            return Some(&[]);
        }
        let entries = slot.aux as usize as *const php_jit::JitNativeDirectArrayEntry;
        if entries.is_null() {
            return None;
        }
        // Direct-array storage lives in the request's stable arena and never
        // relocates. Read-only exact handlers can therefore traverse the
        // authoritative entries in place instead of materializing a second
        // argument/value plane.
        Some(unsafe { std::slice::from_raw_parts(entries, length) })
    }

    fn native_parse_url_direct(
        &mut self,
        input: i64,
        component: Option<i64>,
    ) -> Option<Result<(bool, Option<i64>), i64>> {
        let (input, input_length) = self.stable_native_string_range(input)?;
        // SAFETY: the encoded URL input remains owned for this synchronous
        // parse while the structured publisher writes to disjoint arenas.
        #[allow(unsafe_code)]
        let input = unsafe { std::slice::from_raw_parts(input, input_length) };
        Some(php_runtime::api::native_parse_url_into(
            input, component, self,
        ))
    }

    fn native_file_lines_direct(&mut self, path: i64, flags: i64) -> Option<i64> {
        let (path, path_length) = self.stable_native_string_range(path)?;
        let (cwd, filesystem) = self.native_filesystem_capability()?;
        let cwd = std::ptr::from_ref(cwd);
        let filesystem = std::ptr::from_ref(filesystem);
        // SAFETY: all three owners are request-stable for this synchronous
        // call. Publication mutates only the disjoint native value arenas.
        #[allow(unsafe_code)]
        unsafe {
            php_runtime::api::native_file_lines_into(
                &*cwd,
                &*filesystem,
                std::slice::from_raw_parts(path, path_length),
                flags,
                self,
            )
        }
    }

    fn native_glob_direct(
        &mut self,
        pattern: i64,
    ) -> Option<php_runtime::api::NativeGlobPublished<i64>> {
        let (pattern, pattern_length) = self.stable_native_string_range(pattern)?;
        let (cwd, filesystem) = self.native_filesystem_capability()?;
        let cwd = std::ptr::from_ref(cwd);
        let filesystem = std::ptr::from_ref(filesystem);
        // SAFETY: the pattern and capability owners remain request-stable for
        // this synchronous call while publication touches disjoint arenas.
        #[allow(unsafe_code)]
        unsafe {
            php_runtime::api::native_glob_into(
                &*cwd,
                &*filesystem,
                std::slice::from_raw_parts(pattern, pattern_length),
                self,
            )
        }
    }

    fn native_scandir_direct(
        &mut self,
        path: i64,
        descending: bool,
    ) -> Option<php_runtime::api::NativeGlobPublished<i64>> {
        let (path, path_length) = self.stable_native_string_range(path)?;
        let (cwd, filesystem) = self.native_filesystem_capability()?;
        let cwd = std::ptr::from_ref(cwd);
        let filesystem = std::ptr::from_ref(filesystem);
        // SAFETY: the path and capability owners remain stable while direct
        // native array publication writes only to disjoint arenas.
        #[allow(unsafe_code)]
        unsafe {
            php_runtime::api::native_scandir_into(
                &*cwd,
                &*filesystem,
                std::slice::from_raw_parts(path, path_length),
                descending,
                self,
            )
        }
    }

    fn native_input_key_matches(
        &self,
        encoded: i64,
        key: &php_runtime::api::NativeInputKey,
    ) -> Option<bool> {
        match key {
            php_runtime::api::NativeInputKey::Int(key) => Some(matches!(
                self.native_printf_scalar(encoded),
                Some(php_runtime::api::NativePrintfScalar::Int(candidate)) if candidate == *key
            )),
            php_runtime::api::NativeInputKey::String(key) => Some(matches!(
                self.native_printf_scalar(encoded),
                Some(php_runtime::api::NativePrintfScalar::String(candidate))
                    if candidate == key
            )),
        }
    }

    fn publish_native_input_key(
        &mut self,
        key: &php_runtime::api::NativeInputKey,
    ) -> Result<i64, &'static str> {
        match key {
            php_runtime::api::NativeInputKey::Int(value) => Ok(*value),
            php_runtime::api::NativeInputKey::String(bytes) => {
                self.publish_direct_string_bytes(bytes)
            }
        }
    }

    fn publish_empty_owned_direct_array(&mut self) -> Result<i64, &'static str> {
        self.publish_owned_direct_array_with(0, |_, _| {
            Err("empty native input array requested an entry")
        })
    }

    fn insert_native_input_value(
        &mut self,
        array: i64,
        segments: &[php_runtime::api::NativeInputSegment],
        bytes: &[u8],
    ) -> Result<(), &'static str> {
        let (head, tail) = segments.split_first().ok_or("native input path is empty")?;
        let mut child = None;
        self.mutate_owned_direct_array(array, |fast, writer| {
            let (key, existing) = match head {
                php_runtime::api::NativeInputSegment::Key(key) => {
                    let mut existing = None;
                    for index in 0..writer.len() {
                        let entry = writer
                            .get(index)
                            .ok_or("native input entry disappeared during lookup")?;
                        if fast
                            .native_input_key_matches(entry.key, key)
                            .ok_or("native input key is not directly comparable")?
                        {
                            existing = Some(index);
                            break;
                        }
                    }
                    (key.clone(), existing)
                }
                php_runtime::api::NativeInputSegment::Append => {
                    let mut next = 0_i64;
                    for index in 0..writer.len() {
                        let entry = writer
                            .get(index)
                            .ok_or("native input entry disappeared during append lookup")?;
                        if let Some(php_runtime::api::NativePrintfScalar::Int(candidate)) =
                            fast.native_printf_scalar(entry.key)
                        {
                            next = next.max(candidate.saturating_add(1));
                        }
                    }
                    (php_runtime::api::NativeInputKey::Int(next), None)
                }
            };

            if tail.is_empty() {
                let value = fast.publish_direct_string_bytes(bytes)?;
                if let Some(index) = existing {
                    let current = writer
                        .get(index)
                        .ok_or("native input replacement target disappeared")?;
                    let previous = writer
                        .replace_owned(
                            index,
                            php_jit::JitNativeDirectArrayEntry {
                                key: current.key,
                                value,
                            },
                        )
                        .ok_or("native input replacement failed")?;
                    fast.discard_owned_direct_value(previous.value)?;
                    return Ok(());
                }
                let key = match fast.publish_native_input_key(&key) {
                    Ok(key) => key,
                    Err(error) => {
                        let _ = fast.discard_owned_direct_value(value);
                        return Err(error);
                    }
                };
                let entry = php_jit::JitNativeDirectArrayEntry { key, value };
                if let Err(error) = fast.push_owned_direct_array_entry(writer, entry) {
                    let _ = fast.discard_owned_direct_value(value);
                    let _ = fast.discard_owned_direct_value(key);
                    return Err(error);
                }
                return Ok(());
            }

            if let Some(index) = existing {
                let current = writer
                    .get(index)
                    .ok_or("native input child target disappeared")?;
                if fast.direct_slot(current.value).is_some_and(|(_, slot)| {
                    slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
                }) {
                    child = Some(current.value);
                    return Ok(());
                }
                let replacement = fast.publish_empty_owned_direct_array()?;
                let previous = writer
                    .replace_owned(
                        index,
                        php_jit::JitNativeDirectArrayEntry {
                            key: current.key,
                            value: replacement,
                        },
                    )
                    .ok_or("native input child replacement failed")?;
                fast.discard_owned_direct_value(previous.value)?;
                child = Some(replacement);
                return Ok(());
            }

            let value = fast.publish_empty_owned_direct_array()?;
            let key = match fast.publish_native_input_key(&key) {
                Ok(key) => key,
                Err(error) => {
                    let _ = fast.discard_owned_direct_value(value);
                    return Err(error);
                }
            };
            let entry = php_jit::JitNativeDirectArrayEntry { key, value };
            if let Err(error) = fast.push_owned_direct_array_entry(writer, entry) {
                let _ = fast.discard_owned_direct_value(value);
                let _ = fast.discard_owned_direct_value(key);
                return Err(error);
            }
            child = Some(value);
            Ok(())
        })?;
        if let Some(child) = child {
            self.insert_native_input_value(child, tail, bytes)?;
        }
        Ok(())
    }

    fn native_parse_str_direct(&mut self, input: i64) -> Option<i64> {
        let (input, input_length) = self.stable_native_string_range(input)?;
        let ini = self.native_input_ini_options()?;
        let root = self.publish_empty_owned_direct_array().ok()?;
        // SAFETY: the input owner remains live while parsing mutates only
        // disjoint direct string and array arenas.
        #[allow(unsafe_code)]
        let input = unsafe { std::slice::from_raw_parts(input, input_length) };
        let parsed = php_runtime::api::native_parse_str_into(input, &ini, |segments, value| {
            self.insert_native_input_value(root, segments, value)
        });
        if parsed.is_err() {
            let _ = self.discard_owned_direct_value(root);
            return None;
        }
        Some(root)
    }

    pub(crate) fn native_http_build_query(
        &mut self,
        input: i64,
        numeric_prefix: Option<&[u8]>,
        separator: &[u8],
        raw_encoding: bool,
    ) -> Option<i64> {
        let mut length = 0_usize;
        let mut traversal = NativeHttpQueryTraversal::new();
        let mut emitted = false;
        self.visit_native_http_query_array(
            input,
            numeric_prefix,
            separator,
            &mut traversal,
            &mut emitted,
            &mut |bytes, encoded| {
                let chunk_length = if encoded {
                    php_runtime::api::native_url_encode_output_length(bytes, raw_encoding)?
                } else {
                    bytes.len()
                };
                length = length.checked_add(chunk_length)?;
                Some(())
            },
        )?;
        let state = std::ptr::from_ref(self);
        self.try_publish_direct_string_with(length, |output| {
            let mut traversal = NativeHttpQueryTraversal::new();
            let mut emitted = false;
            let mut cursor = 0_usize;
            // SAFETY: the request state and every native arena base remain
            // stable during this synchronous publication.
            #[allow(unsafe_code)]
            let state = unsafe { &*state };
            state
                .visit_native_http_query_array(
                    input,
                    numeric_prefix,
                    separator,
                    &mut traversal,
                    &mut emitted,
                    &mut |bytes, encoded| {
                        if encoded {
                            let chunk_length = php_runtime::api::native_url_encode_output_length(
                                bytes,
                                raw_encoding,
                            )?;
                            let end = cursor.checked_add(chunk_length)?;
                            php_runtime::api::native_url_encode_into(
                                bytes,
                                raw_encoding,
                                output.get_mut(cursor..end)?,
                            )
                            .then_some(())?;
                            cursor = end;
                        } else {
                            let end = cursor.checked_add(bytes.len())?;
                            output.get_mut(cursor..end)?.copy_from_slice(bytes);
                            cursor = end;
                        }
                        Some(())
                    },
                )
                .filter(|_| cursor == output.len())
                .ok_or("native HTTP query writer length contract failed")
        })
        .ok()
    }

    fn native_sort_numeric(&self, encoded: i64) -> Option<f64> {
        match self.native_printf_scalar(encoded)? {
            php_runtime::api::NativePrintfScalar::Null
            | php_runtime::api::NativePrintfScalar::Bool(false) => Some(0.0),
            php_runtime::api::NativePrintfScalar::Bool(true) => Some(1.0),
            php_runtime::api::NativePrintfScalar::Int(value) => Some(value as f64),
            php_runtime::api::NativePrintfScalar::Float(value) => Some(value),
            php_runtime::api::NativePrintfScalar::String(value) => Some(
                std::str::from_utf8(value)
                    .ok()
                    .and_then(|value| value.parse::<f64>().ok())
                    .unwrap_or(0.0),
            ),
        }
    }

    fn native_sort_order<
        const COMPARE_KEYS: bool,
        const REVERSE: bool,
        const FIXED_NATURAL: bool,
        const FORCE_CASE_INSENSITIVE: bool,
    >(
        &self,
        left: php_jit::JitNativeDirectArrayEntry,
        right: php_jit::JitNativeDirectArrayEntry,
        flags: i64,
    ) -> Option<std::cmp::Ordering> {
        let natural = FIXED_NATURAL || flags & !8 == 6;
        let case_insensitive = FORCE_CASE_INSENSITIVE || flags & 8 != 0;
        let left = if COMPARE_KEYS { left.key } else { left.value };
        let right = if COMPARE_KEYS { right.key } else { right.value };
        let ordering = if natural {
            let left = self.native_scalar_bytes(left)?;
            let right = self.native_scalar_bytes(right)?;
            php_runtime::api::native_natural_compare(
                left.as_bytes(),
                right.as_bytes(),
                case_insensitive,
            )
            .cmp(&0)
        } else if flags & !8 == 1 {
            self.native_sort_numeric(left)?
                .partial_cmp(&self.native_sort_numeric(right)?)?
        } else if flags & !8 == 2 || COMPARE_KEYS {
            let left = self.native_scalar_bytes(left)?;
            let right = self.native_scalar_bytes(right)?;
            if case_insensitive {
                left.as_bytes()
                    .iter()
                    .map(|byte| byte.to_ascii_lowercase())
                    .cmp(
                        right
                            .as_bytes()
                            .iter()
                            .map(|byte| byte.to_ascii_lowercase()),
                    )
            } else {
                left.as_bytes().cmp(right.as_bytes())
            }
        } else if flags & !8 == 0 {
            self.native_values_compare(left, right, &mut NativeComparisonTraversal::default())?
        } else {
            return None;
        };
        Some(if REVERSE {
            ordering.reverse()
        } else {
            ordering
        })
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_sort<
        const COMPARE_KEYS: bool,
        const REVERSE: bool,
        const FIXED_NATURAL: bool,
        const FORCE_CASE_INSENSITIVE: bool,
        const PRESERVE_KEYS: bool,
    >(
        &mut self,
        reference: i64,
        flags: i64,
    ) -> Option<()> {
        let (_, reference_slot) = self.direct_slot(reference)?;
        if reference_slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            || reference_slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
        {
            return None;
        }
        let array = reference_slot.payload as i64;
        let (array_index, array_slot) = self.direct_slot(array)?;
        if array_slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
            || array_slot.refcount != 1
        {
            return None;
        }
        let length = usize::try_from(array_slot.payload).ok()?;
        let entries = array_slot.aux as usize as *mut php_jit::JitNativeDirectArrayEntry;
        if length != 0 && entries.is_null() {
            return None;
        }
        // Build only a compact permutation, not a duplicate entry/value
        // plane. All comparisons complete before the authoritative arena is
        // mutated, so an unsupported PHP comparison can still take the one
        // baseline continuation with the original array untouched.
        let mut order = (0..length)
            .map(u32::try_from)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        for index in 1..order.len() {
            let mut cursor = index;
            while cursor > 0 {
                let left = usize::try_from(order[cursor - 1]).ok()?;
                let right = usize::try_from(order[cursor]).ok()?;
                let ordering = self.native_sort_order::<
                    COMPARE_KEYS,
                    REVERSE,
                    FIXED_NATURAL,
                    FORCE_CASE_INSENSITIVE,
                >(
                    unsafe { *entries.add(left) },
                    unsafe { *entries.add(right) },
                    flags,
                )?;
                if !ordering.is_gt() {
                    break;
                }
                order.swap(cursor - 1, cursor);
                cursor -= 1;
            }
        }

        // Convert the destination-to-current-position permutation into the
        // final in-place layout. Updating the remaining positions after each
        // swap keeps the plan valid without a second visited/current map.
        for destination in 0..order.len() {
            let source = usize::try_from(order[destination]).ok()?;
            if source == destination {
                continue;
            }
            unsafe { std::ptr::swap(entries.add(destination), entries.add(source)) };
            order[destination] = u32::try_from(destination).ok()?;
            for current in &mut order[destination + 1..] {
                let current_index = usize::try_from(*current).ok()?;
                if current_index == destination {
                    *current = u32::try_from(source).ok()?;
                } else if current_index == source {
                    *current = u32::try_from(destination).ok()?;
                }
            }
        }
        if !PRESERVE_KEYS {
            for index in 0..length {
                let key = unsafe { (*entries.add(index)).key };
                let _ = self.discard_owned_direct_value(key);
                unsafe {
                    (*entries.add(index)).key = i64::try_from(index).ok()?;
                }
            }
        }
        let slots = self.header.active_runtime_view().direct_value_slots as usize
            as *mut php_jit::JitNativeValueSlot;
        unsafe {
            (*slots.add(array_index)).flags =
                php_jit::jit_native_direct_array_flags((length != 0).then_some(0));
        }
        Some(())
    }

    /// Coordinates PHP's variadic `array_multisort` directly over the
    /// authoritative entry arenas. The only temporary value is one compact
    /// permutation; comparisons finish before any PHP-visible array mutates.
    #[allow(unsafe_code)] // Safety: every pointer is validated against request-owned direct slots before synchronous use.
    fn native_array_multisort(&mut self, arguments: &[i64]) -> Option<()> {
        struct NativeMultisortArray {
            slot_index: usize,
            entries: *mut php_jit::JitNativeDirectArrayEntry,
            length: usize,
            flags: i64,
            reverse: bool,
            direction_seen: bool,
            mode_seen: bool,
        }

        let mut arrays = Vec::<NativeMultisortArray>::new();
        for argument in arguments {
            let mut value = *argument;
            let mut passed_by_reference = false;
            if let Some((_, slot)) = self.direct_slot(value)
                && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                && native_reference_state(slot.reserved)
                    != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
            {
                value = slot.payload as i64;
                passed_by_reference = true;
            }

            if let Some((slot_index, slot)) = self.direct_slot(value)
                && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
            {
                if !passed_by_reference
                    || slot.refcount != 1
                    || arrays.iter().any(|array| array.slot_index == slot_index)
                {
                    return None;
                }
                let length = usize::try_from(slot.payload).ok()?;
                let entries = slot.aux as usize as *mut php_jit::JitNativeDirectArrayEntry;
                if length != 0 && entries.is_null() {
                    return None;
                }
                if arrays.first().is_some_and(|array| array.length != length) {
                    return None;
                }
                arrays.push(NativeMultisortArray {
                    slot_index,
                    entries,
                    length,
                    flags: 0,
                    reverse: false,
                    direction_seen: false,
                    mode_seen: false,
                });
                continue;
            }

            let flag = match self.native_printf_scalar(value)? {
                php_runtime::api::NativePrintfScalar::Int(value) => value,
                _ => return None,
            };
            let current = arrays.last_mut()?;
            match flag {
                3 | 4 if !current.direction_seen => {
                    current.reverse = flag == 3;
                    current.direction_seen = true;
                }
                0 | 1 | 2 | 6 | 8 | 10 | 14 if !current.mode_seen => {
                    current.flags = flag;
                    current.mode_seen = true;
                }
                _ => return None,
            }
        }
        let length = arrays.first()?.length;

        // Keys are rewritten after permutation. Prove their direct scalar
        // shape before comparison so the mutation phase cannot discover a
        // late unsupported key and request baseline after partial mutation.
        for array in &arrays {
            for index in 0..array.length {
                let key = unsafe { (*array.entries.add(index)).key };
                match self.native_printf_scalar(key)? {
                    php_runtime::api::NativePrintfScalar::Int(_)
                    | php_runtime::api::NativePrintfScalar::String(_) => {}
                    _ => return None,
                }
            }
        }

        let mut order = (0..length)
            .map(u32::try_from)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        for index in 1..order.len() {
            let mut cursor = index;
            while cursor > 0 {
                let left = usize::try_from(order[cursor - 1]).ok()?;
                let right = usize::try_from(order[cursor]).ok()?;
                let mut ordering = std::cmp::Ordering::Equal;
                for array in &arrays {
                    let left = unsafe { *array.entries.add(left) };
                    let right = unsafe { *array.entries.add(right) };
                    ordering = if array.reverse {
                        self.native_sort_order::<false, true, false, false>(
                            left,
                            right,
                            array.flags,
                        )?
                    } else {
                        self.native_sort_order::<false, false, false, false>(
                            left,
                            right,
                            array.flags,
                        )?
                    };
                    if !ordering.is_eq() {
                        break;
                    }
                }
                if !ordering.is_gt() {
                    break;
                }
                order.swap(cursor - 1, cursor);
                cursor -= 1;
            }
        }

        // Turn the destination-to-source plan into the final layout once and
        // apply each planned swap to every participating array.
        for destination in 0..order.len() {
            let source = usize::try_from(order[destination]).ok()?;
            if source != destination {
                for array in &arrays {
                    unsafe {
                        std::ptr::swap(array.entries.add(destination), array.entries.add(source));
                    }
                }
                order[destination] = u32::try_from(destination).ok()?;
                for current in &mut order[destination + 1..] {
                    let current_index = usize::try_from(*current).ok()?;
                    if current_index == destination {
                        *current = u32::try_from(source).ok()?;
                    } else if current_index == source {
                        *current = u32::try_from(destination).ok()?;
                    }
                }
            }
        }

        for array in arrays {
            let mut next_integer = 0_u32;
            for index in 0..array.length {
                let entry = unsafe { &mut *array.entries.add(index) };
                if matches!(
                    self.native_printf_scalar(entry.key)?,
                    php_runtime::api::NativePrintfScalar::Int(_)
                ) {
                    let previous = entry.key;
                    entry.key = i64::from(next_integer);
                    next_integer = next_integer.checked_add(1)?;
                    self.discard_owned_direct_value(previous).ok()?;
                }
            }
            let slots = self.header.active_runtime_view().direct_value_slots as usize
                as *mut php_jit::JitNativeValueSlot;
            unsafe {
                (*slots.add(array.slot_index)).flags = php_jit::jit_native_direct_array_flags(
                    (next_integer != 0).then_some(next_integer),
                );
            }
        }
        Some(())
    }

    fn emit_native_http_query_path(
        &self,
        path: &[NativeHttpQueryPathSegment],
        numeric_prefix: Option<&[u8]>,
        emit: &mut impl FnMut(&[u8], bool) -> Option<()>,
    ) -> Option<()> {
        let mut integer = [0; 20];
        for (index, segment) in path.iter().copied().enumerate() {
            if index != 0 {
                emit(b"[", true)?;
            } else if matches!(segment, NativeHttpQueryPathSegment::Integer(_))
                && let Some(numeric_prefix) = numeric_prefix
            {
                emit(numeric_prefix, true)?;
            }
            match segment {
                NativeHttpQueryPathSegment::Integer(value) => {
                    emit(native_i64_ascii(value, &mut integer), true)?;
                }
                NativeHttpQueryPathSegment::String(bytes, length) => {
                    // SAFETY: path segments point into stable live native
                    // strings owned for the synchronous traversal.
                    #[allow(unsafe_code)]
                    emit(unsafe { std::slice::from_raw_parts(bytes, length) }, true)?;
                }
            }
            if index != 0 {
                emit(b"]", true)?;
            }
        }
        Some(())
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_native_http_query_array(
        &self,
        encoded: i64,
        numeric_prefix: Option<&[u8]>,
        separator: &[u8],
        traversal: &mut NativeHttpQueryTraversal,
        emitted: &mut bool,
        emit: &mut impl FnMut(&[u8], bool) -> Option<()>,
    ) -> Option<()> {
        let encoded = self.native_by_value_encoding(encoded)?;
        let (array_index, slot) = self.direct_slot(encoded)?;
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
            || traversal.array_is_active(array_index)
        {
            return None;
        }
        traversal.push_array(array_index)?;
        let entries = self.native_direct_array_entries(encoded)?;
        for entry in entries {
            let key = match self.native_printf_scalar(entry.key)? {
                php_runtime::api::NativePrintfScalar::Int(value) => {
                    NativeHttpQueryPathSegment::Integer(value)
                }
                php_runtime::api::NativePrintfScalar::String(value) => {
                    NativeHttpQueryPathSegment::String(value.as_ptr(), value.len())
                }
                _ => return None,
            };
            traversal.push_path(key)?;
            let value = self.native_by_value_encoding(entry.value)?;
            if let Some((_, value_slot)) = self.direct_slot(value) {
                if value_slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY {
                    self.visit_native_http_query_array(
                        value,
                        numeric_prefix,
                        separator,
                        traversal,
                        emitted,
                        emit,
                    )?;
                    traversal.pop_path();
                    continue;
                }
                if value_slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE {
                    traversal.pop_path();
                    continue;
                }
            }
            let scalar = self.native_printf_scalar(value)?;
            if matches!(scalar, php_runtime::api::NativePrintfScalar::Null) {
                traversal.pop_path();
                continue;
            }
            if *emitted {
                emit(separator, false)?;
            }
            self.emit_native_http_query_path(traversal.path(), numeric_prefix, emit)?;
            emit(b"=", false)?;
            let mut integer = [0; 20];
            let mut float = [0; php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY];
            match scalar {
                php_runtime::api::NativePrintfScalar::Null => unreachable!(),
                php_runtime::api::NativePrintfScalar::Bool(value) => {
                    emit(if value { b"1" } else { b"0" }, true)?;
                }
                php_runtime::api::NativePrintfScalar::Int(value) => {
                    emit(native_i64_ascii(value, &mut integer), true)?;
                }
                php_runtime::api::NativePrintfScalar::Float(value) => {
                    emit(
                        php_runtime::api::float_to_php_string_bytes(value, &mut float),
                        true,
                    )?;
                }
                php_runtime::api::NativePrintfScalar::String(value) => emit(value, true)?,
            }
            *emitted = true;
            traversal.pop_path();
        }
        traversal.pop_array();
        Some(())
    }

    fn native_arg_separator_output(&self) -> Option<&str> {
        self.configuration
            .ini_registry()
            .get("arg_separator.output")
            .or(Some("&"))
    }

    fn native_input_ini_options(&self) -> Option<php_runtime::api::RuntimeIniOptions> {
        let registry = self.configuration.ini_registry();
        let mut ini = php_runtime::api::RuntimeIniOptions::default();
        if let Some(value) = registry.get("arg_separator.input") {
            ini.arg_separator_input = value.to_owned();
        }
        if let Some(value) = registry.get("max_input_vars")
            && let Ok(limit) = value.parse::<usize>()
        {
            ini.max_input_vars = limit;
        }
        if let Some(value) = registry.get("max_input_nesting_level")
            && let Ok(limit) = value.parse::<usize>()
        {
            ini.max_input_nesting_level = limit;
        }
        if let Some(value) = registry.get("filter.default")
            && let Some(filter) = php_runtime::api::RuntimeInputFilter::from_ini_value(value)
        {
            ini.default_input_filter = filter;
        }
        if let Some(value) = registry.get("filter.default_flags")
            && let Ok(flags) = value.parse::<i64>()
        {
            ini.default_input_filter_flags = flags;
        }
        Some(ini)
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_preg_replace_many_direct(
        &mut self,
        pattern: i64,
        replacement: i64,
        input: i64,
        limit: i64,
        filter: bool,
    ) -> Option<(i64, i64)> {
        let (entries, length) = self.stable_native_array_range(input)?;
        for index in 0..length {
            let entry = unsafe { *entries.add(index) };
            self.stable_native_string_range(entry.value)?;
        }
        let limits = self.native_pcre_limits()?;
        let state = self.pcre_state;
        let (pattern, pattern_length) = self.stable_native_string_range(pattern)?;
        let (replacement, replacement_length) = self.stable_native_string_range(replacement)?;
        let pattern = unsafe { std::slice::from_raw_parts(pattern, pattern_length) };
        let replacement = unsafe { std::slice::from_raw_parts(replacement, replacement_length) };
        let state = unsafe { state.as_mut() }?;
        let fast = self as *mut Self;
        let mut writer = self
            .begin_owned_direct_array(4, php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY)
            .ok()?;
        let result = php_runtime::api::native_preg_replace_many_into(
            state,
            limits,
            pattern,
            replacement,
            length,
            |source_index| {
                let entry = unsafe { *entries.add(source_index) };
                let (bytes, length) = unsafe { (&*fast).stable_native_string_range(entry.value) }?;
                Some(unsafe { std::slice::from_raw_parts(bytes, length) })
            },
            limit,
            filter,
            |source_index, bytes| {
                let Some(bytes) = bytes else {
                    return Ok(());
                };
                if source_index >= length {
                    return Err("native preg_replace output exceeds its source");
                }
                let entry = unsafe { *entries.add(source_index) };
                self.retain_direct_encoded(entry.key)?;
                let value = match self.publish_direct_string_bytes(bytes) {
                    Ok(value) => value,
                    Err(error) => {
                        self.rollback_direct_retain(entry.key);
                        return Err(error);
                    }
                };
                if let Err(error) = self.push_owned_direct_array_entry(
                    &mut writer,
                    php_jit::JitNativeDirectArrayEntry {
                        key: entry.key,
                        value,
                    },
                ) {
                    let _ = self.discard_owned_direct_value(value);
                    self.rollback_direct_retain(entry.key);
                    return Err(error);
                }
                Ok(())
            },
        );
        let count = match result {
            Ok(Some(count)) => count,
            Ok(None) | Err(_) => {
                self.abort_owned_direct_array(writer);
                return None;
            }
        };
        let output = self.finish_owned_direct_array(writer).ok()?;
        Some((output, count))
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_preg_grep_direct(&mut self, pattern: i64, input: i64, flags: i64) -> Option<i64> {
        let (entries, length) = self.stable_native_array_range(input)?;
        for index in 0..length {
            let entry = unsafe { *entries.add(index) };
            self.stable_native_string_range(entry.value)?;
        }
        let limits = self.native_pcre_limits()?;
        let state = self.pcre_state;
        let (pattern, pattern_length) = self.stable_native_string_range(pattern)?;
        let pattern = unsafe { std::slice::from_raw_parts(pattern, pattern_length) };
        let state = unsafe { state.as_mut() }?;
        let fast = self as *mut Self;
        let mut writer = self
            .begin_owned_direct_array(4, php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY)
            .ok()?;
        let result = php_runtime::api::native_preg_grep_into(
            state,
            limits,
            pattern,
            length,
            |source_index| {
                let entry = unsafe { *entries.add(source_index) };
                let (bytes, length) = unsafe { (&*fast).stable_native_string_range(entry.value) }?;
                Some(unsafe { std::slice::from_raw_parts(bytes, length) })
            },
            flags,
            |source_index| {
                if source_index >= length {
                    return Err("native preg_grep output exceeds its source");
                }
                let entry = unsafe { *entries.add(source_index) };
                self.retain_direct_encoded(entry.key)?;
                if let Err(error) = self.retain_direct_encoded(entry.value) {
                    self.rollback_direct_retain(entry.key);
                    return Err(error);
                }
                if let Err(error) = self.push_owned_direct_array_entry(&mut writer, entry) {
                    self.rollback_direct_retain(entry.value);
                    self.rollback_direct_retain(entry.key);
                    return Err(error);
                }
                Ok(())
            },
        );
        match result {
            Ok(Some(())) => self.finish_owned_direct_array(writer).ok(),
            Ok(None) | Err(_) => {
                self.abort_owned_direct_array(writer);
                None
            }
        }
    }

    fn native_pcre_limits(&self) -> Option<php_runtime::api::PcreMatchLimits> {
        let ini = self.configuration.ini_registry();
        Some(php_runtime::api::PcreMatchLimits {
            backtrack_limit: ini
                .get("pcre.backtrack_limit")
                .and_then(|value| value.trim().parse().ok()),
            recursion_limit: ini
                .get("pcre.recursion_limit")
                .and_then(|value| value.trim().parse().ok()),
            jit: ini
                .get("pcre.jit")
                .is_none_or(|value| !matches!(value.trim(), "" | "0" | "Off" | "off" | "false")),
        })
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn direct_reference_accepts_native_replace(&self, reference: i64) -> bool {
        let Some((_, slot)) = self.direct_slot(reference) else {
            return false;
        };
        slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            && self.direct_owner_tree_is_discardable(slot.payload as i64, 0)
    }

    fn direct_owner_tree_is_discardable(&self, encoded: i64, depth: usize) -> bool {
        if depth >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
            return false;
        }
        let Some((_, slot)) = self.direct_slot(encoded) else {
            return php_jit::jit_decode_runtime_value(encoded).is_none();
        };
        if slot.refcount > 1 {
            return true;
        }
        match slot.kind {
            php_jit::JIT_NATIVE_VALUE_VIEW_STRING
                if slot.flags == php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION =>
            {
                true
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
                if slot.flags == php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION =>
            {
                true
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => true,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY => {
                let Some(entries) = self.native_direct_array_entries(encoded) else {
                    return false;
                };
                entries.iter().all(|entry| {
                    self.direct_owner_tree_is_discardable(entry.key, depth + 1)
                        && self.direct_owner_tree_is_discardable(entry.value, depth + 1)
                })
            }
            _ => false,
        }
    }

    /// Transfers a freshly published exact result into a direct reference.
    /// The prior owner is retired entirely in the native plane when its graph
    /// is destructor-free; object/cold graphs retain the one baseline
    /// continuation so PHP-visible destruction cannot be skipped.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn replace_direct_reference(&mut self, reference: i64, value: i64) -> bool {
        let Some((index, slot)) = self.direct_slot(reference) else {
            let _ = self.discard_owned_direct_value(value);
            return false;
        };
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            || slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
        {
            let _ = self.discard_owned_direct_value(value);
            return false;
        }
        let previous = slot.payload as i64;
        if !self.direct_owner_tree_is_discardable(previous, 0) {
            let _ = self.discard_owned_direct_value(value);
            return false;
        }
        let slots = self.header.active_runtime_view().direct_value_slots as usize
            as *mut php_jit::JitNativeValueSlot;
        unsafe {
            (*slots.add(index)).payload = value as u64;
            (*slots.add(index)).reserved = php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED
                | (slot.reserved & php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD);
        }
        self.discard_owned_direct_value(previous).is_ok()
    }

    pub(crate) fn native_session_payload(&self) -> Option<i64> {
        let (_, slot) = self.direct_slot(self.session.global_reference)?;
        (slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            && native_reference_state(slot.reserved)
                != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY)
            .then_some(slot.payload as i64)
    }

    fn native_session_payload_is_array(&self, encoded: i64) -> bool {
        self.native_direct_array_entries(encoded).is_some()
    }

    /// Commits the current authoritative `$_SESSION` payload by adding one
    /// native owner. A later write sees the shared array refcount and performs
    /// COW in the same data plane.
    pub(crate) fn commit_native_session_payload(&mut self) -> bool {
        let Some(current) = self.native_session_payload() else {
            return false;
        };
        if !self.native_session_payload_is_array(current)
            || !self.direct_owner_tree_is_discardable(current, 0)
            || !self.direct_owner_tree_is_discardable(self.session.committed, 0)
            || self.retain_direct_encoded(current).is_err()
        {
            return false;
        }
        let previous = std::mem::replace(&mut self.session.committed, current);
        if self.discard_owned_direct_value(previous).is_err() {
            self.rollback_direct_retain(current);
            self.session.committed = previous;
            return false;
        }
        true
    }

    /// Restores the committed native COW owner into the canonical global
    /// reference without constructing or cloning a Rust `Value`.
    fn restore_native_session_payload(&mut self) -> bool {
        let committed = self.session.committed;
        let Some(current) = self.native_session_payload() else {
            return false;
        };
        if !self.native_session_payload_is_array(committed)
            || !self.direct_owner_tree_is_discardable(current, 0)
            || self.retain_direct_encoded(committed).is_err()
        {
            return false;
        }
        if self.replace_direct_reference(self.session.global_reference, committed) {
            true
        } else {
            self.rollback_direct_retain(committed);
            false
        }
    }

    pub(crate) fn replace_native_session_payload_owned(&mut self, value: i64) -> bool {
        self.native_session_payload_is_array(value)
            && self.replace_direct_reference(self.session.global_reference, value)
    }

    fn clear_native_session_payload(&mut self) -> bool {
        let Some(current) = self.native_session_payload() else {
            return false;
        };
        if !self.direct_owner_tree_is_discardable(current, 0) {
            return false;
        }
        let Ok(empty) = self.publish_owned_direct_array_from_iter(std::iter::empty()) else {
            return false;
        };
        self.replace_native_session_payload_owned(empty)
    }

    fn clear_native_session_payload_and_commit(&mut self) -> bool {
        let Some(current) = self.native_session_payload() else {
            return false;
        };
        let committed = self.session.committed;
        if !self.direct_owner_tree_is_discardable(current, 0)
            || !self.direct_owner_tree_is_discardable(committed, 0)
        {
            return false;
        }
        let Ok(empty_payload) = self.publish_owned_direct_array_from_iter(std::iter::empty())
        else {
            return false;
        };
        let Ok(empty_commit) = self.publish_owned_direct_array_from_iter(std::iter::empty()) else {
            let _ = self.discard_owned_direct_value(empty_payload);
            return false;
        };
        if !self.replace_native_session_payload_owned(empty_payload) {
            let _ = self.discard_owned_direct_value(empty_commit);
            return false;
        }
        let previous = std::mem::replace(&mut self.session.committed, empty_commit);
        self.discard_owned_direct_value(previous).is_ok()
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn clear_json_error(&mut self) -> Result<(), &'static str> {
        self.set_json_error(0)
    }

    #[allow(unsafe_code)] // Safety: the native request owns the published JSON state.
    fn set_json_error(&mut self, code: i64) -> Result<(), &'static str> {
        let state =
            unsafe { self.json_state.as_mut() }.ok_or("native JSON state is unavailable")?;
        state.set(code);
        Ok(())
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    pub(crate) fn native_printf_scalar(
        &self,
        encoded: i64,
    ) -> Option<php_runtime::api::NativePrintfScalar<'_>> {
        let encoded = self.native_by_value_encoding(encoded)?;
        if let Some((_, slot)) = self.direct_slot(encoded) {
            return match slot.kind {
                php_jit::JIT_NATIVE_VALUE_VIEW_STRING => self
                    .native_string_view(encoded)
                    .map(php_runtime::api::NativePrintfScalar::String),
                php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => Some(
                    php_runtime::api::NativePrintfScalar::Float(f64::from_bits(slot.payload)),
                ),
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
                    if slot.flags == php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION =>
                {
                    Some(php_runtime::api::NativePrintfScalar::Int(
                        slot.payload as i64,
                    ))
                }
                _ => None,
            };
        }
        if php_jit::jit_decode_runtime_value(encoded).is_none()
            && php_jit::jit_decode_constant(encoded).is_none()
        {
            return Some(php_runtime::api::NativePrintfScalar::Int(encoded));
        }
        let constant = php_jit::jit_decode_constant(encoded)?;
        if constant == u32::MAX {
            return Some(php_runtime::api::NativePrintfScalar::Null);
        }
        if constant == php_jit::JIT_VALUE_FALSE {
            return Some(php_runtime::api::NativePrintfScalar::Bool(false));
        }
        if constant == php_jit::JIT_VALUE_TRUE {
            return Some(php_runtime::api::NativePrintfScalar::Bool(true));
        }
        let view = self.header.active_runtime_view();
        if constant >= view.trusted_constant_view_count {
            return None;
        }
        let constants =
            view.trusted_constant_views as usize as *const php_jit::JitNativeConstantView;
        let constant = unsafe { *constants.add(constant as usize) };
        match constant.kind {
            php_jit::JIT_NATIVE_CONSTANT_VIEW_NULL => {
                Some(php_runtime::api::NativePrintfScalar::Null)
            }
            php_jit::JIT_NATIVE_CONSTANT_VIEW_BOOL => Some(
                php_runtime::api::NativePrintfScalar::Bool(constant.length != 0),
            ),
            php_jit::JIT_NATIVE_CONSTANT_VIEW_INT => Some(
                php_runtime::api::NativePrintfScalar::Int(constant.length as i64),
            ),
            php_jit::JIT_NATIVE_CONSTANT_VIEW_FLOAT => Some(
                php_runtime::api::NativePrintfScalar::Float(f64::from_bits(constant.length)),
            ),
            php_jit::JIT_NATIVE_CONSTANT_VIEW_STRING => self
                .native_string_view(encoded)
                .map(php_runtime::api::NativePrintfScalar::String),
            _ => None,
        }
    }

    fn native_scalar_bytes(&self, encoded: i64) -> Option<NativeScalarBytes<'_>> {
        match self.native_printf_scalar(encoded)? {
            php_runtime::api::NativePrintfScalar::Null
            | php_runtime::api::NativePrintfScalar::Bool(false) => Some(NativeScalarBytes::Empty),
            php_runtime::api::NativePrintfScalar::Bool(true) => {
                Some(NativeScalarBytes::Static(b"1"))
            }
            php_runtime::api::NativePrintfScalar::Int(value) => {
                let mut bytes = [0_u8; 20];
                let rendered_length = native_i64_ascii(value, &mut bytes).len();
                let start = bytes.len() - rendered_length;
                Some(NativeScalarBytes::Integer { bytes, start })
            }
            php_runtime::api::NativePrintfScalar::Float(value) => {
                let mut bytes = [0_u8; php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY];
                let length = php_runtime::api::float_to_php_string_bytes(value, &mut bytes).len();
                Some(NativeScalarBytes::Float { bytes, length })
            }
            php_runtime::api::NativePrintfScalar::String(bytes) => {
                Some(NativeScalarBytes::Borrowed(bytes))
            }
        }
    }

    fn native_dereferenced_scalar_encoding(&self, encoded: i64) -> Option<i64> {
        let encoded = self.native_by_value_encoding(encoded)?;
        self.native_printf_scalar(encoded)?;
        Some(encoded)
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_printf_array_entries(
        &self,
        encoded: i64,
    ) -> Option<&[php_jit::JitNativeDirectArrayEntry]> {
        self.native_direct_array_entries(encoded)
    }

    /// Classifies one authoritative encoding for the exact native comparison
    /// handlers. The returned view borrows slot payloads directly and never
    /// constructs a Rust `Value` or consults the cold compatibility plane.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_comparison_value(&self, encoded: i64) -> Option<NativeComparisonValue<'_>> {
        let encoded = self.native_by_value_encoding(encoded)?;
        if let Some((index, slot)) = self.direct_slot(encoded) {
            return match slot.kind {
                php_jit::JIT_NATIVE_VALUE_VIEW_STRING => self
                    .native_string_view(encoded)
                    .map(NativeComparisonValue::String),
                php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => {
                    Some(NativeComparisonValue::Float(f64::from_bits(slot.payload)))
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
                    if slot.flags == php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION =>
                {
                    Some(NativeComparisonValue::Int(slot.payload as i64))
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY => {
                    let length = usize::try_from(slot.payload).ok()?;
                    if length == 0 {
                        return Some(NativeComparisonValue::Array {
                            identity: index,
                            entries: &[],
                        });
                    }
                    let entries = slot.aux as usize as *const php_jit::JitNativeDirectArrayEntry;
                    if entries.is_null() {
                        return None;
                    }
                    Some(NativeComparisonValue::Array {
                        identity: index,
                        entries: unsafe { std::slice::from_raw_parts(entries, length) },
                    })
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT => {
                    let object = self.direct_object(encoded)?;
                    Some(NativeComparisonValue::Object(NativeComparisonObject {
                        identity: object.id(),
                        layout_id: php_jit::jit_native_object_property_view_is_published(
                            slot.flags,
                        )
                        .then_some(slot.payload),
                        owner: object,
                    }))
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE
                    if slot.flags == php_jit::JIT_NATIVE_DIRECT_RESOURCE_ABI_VERSION =>
                {
                    Some(NativeComparisonValue::Resource(slot.payload))
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
                | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
                | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR => {
                    Some(NativeComparisonValue::OpaqueIdentity(encoded as u64))
                }
                _ => None,
            };
        }
        if php_jit::jit_decode_runtime_value(encoded).is_none()
            && php_jit::jit_decode_constant(encoded).is_none()
        {
            return Some(NativeComparisonValue::Int(encoded));
        }
        let constant = php_jit::jit_decode_constant(encoded)?;
        if constant == u32::MAX {
            return Some(NativeComparisonValue::Null);
        }
        if constant == php_jit::JIT_VALUE_FALSE {
            return Some(NativeComparisonValue::Bool(false));
        }
        if constant == php_jit::JIT_VALUE_TRUE {
            return Some(NativeComparisonValue::Bool(true));
        }
        if constant == php_jit::JIT_VALUE_UNINITIALIZED {
            return None;
        }
        let view = self.header.active_runtime_view();
        if constant >= view.trusted_constant_view_count {
            return None;
        }
        let constants =
            view.trusted_constant_views as usize as *const php_jit::JitNativeConstantView;
        let constant = unsafe { *constants.add(constant as usize) };
        match constant.kind {
            php_jit::JIT_NATIVE_CONSTANT_VIEW_NULL => Some(NativeComparisonValue::Null),
            php_jit::JIT_NATIVE_CONSTANT_VIEW_BOOL => {
                Some(NativeComparisonValue::Bool(constant.length != 0))
            }
            php_jit::JIT_NATIVE_CONSTANT_VIEW_INT => {
                Some(NativeComparisonValue::Int(constant.length as i64))
            }
            php_jit::JIT_NATIVE_CONSTANT_VIEW_FLOAT => Some(NativeComparisonValue::Float(
                f64::from_bits(constant.length),
            )),
            php_jit::JIT_NATIVE_CONSTANT_VIEW_STRING => self
                .native_string_view(encoded)
                .map(NativeComparisonValue::String),
            _ => None,
        }
    }

    fn native_values_identical(
        &self,
        left: i64,
        right: i64,
        traversal: &mut NativeComparisonTraversal,
    ) -> Option<bool> {
        let left = self.native_comparison_value(left)?;
        let right = self.native_comparison_value(right)?;
        match (left, right) {
            (NativeComparisonValue::Null, NativeComparisonValue::Null) => Some(true),
            (NativeComparisonValue::Bool(left), NativeComparisonValue::Bool(right)) => {
                Some(left == right)
            }
            (NativeComparisonValue::Int(left), NativeComparisonValue::Int(right)) => {
                Some(left == right)
            }
            (NativeComparisonValue::Float(left), NativeComparisonValue::Float(right)) => {
                Some(left == right)
            }
            (NativeComparisonValue::String(left), NativeComparisonValue::String(right)) => {
                Some(left == right)
            }
            (
                NativeComparisonValue::Array {
                    identity: left_identity,
                    entries: left,
                },
                NativeComparisonValue::Array {
                    identity: right_identity,
                    entries: right,
                },
            ) => {
                self.native_arrays_identical(left_identity, left, right_identity, right, traversal)
            }
            (NativeComparisonValue::Object(left), NativeComparisonValue::Object(right)) => {
                Some(left.identity == right.identity)
            }
            (
                NativeComparisonValue::OpaqueIdentity(left),
                NativeComparisonValue::OpaqueIdentity(right),
            )
            | (NativeComparisonValue::Resource(left), NativeComparisonValue::Resource(right)) => {
                Some(left == right)
            }
            _ => Some(false),
        }
    }

    fn native_arrays_identical(
        &self,
        left_identity: usize,
        left: &[php_jit::JitNativeDirectArrayEntry],
        right_identity: usize,
        right: &[php_jit::JitNativeDirectArrayEntry],
        traversal: &mut NativeComparisonTraversal,
    ) -> Option<bool> {
        if left_identity == right_identity {
            return Some(true);
        }
        if left.len() != right.len() {
            return Some(false);
        }
        let pair = (left_identity, right_identity);
        if traversal.arrays.contains(&pair) {
            return None;
        }
        traversal.arrays.push(pair);
        let result = left
            .iter()
            .zip(right)
            .try_fold(true, |identical, (left, right)| {
                if !identical
                    || self.native_compare_array_keys(left.key, right.key)
                        != Some(std::cmp::Ordering::Equal)
                {
                    return Some(false);
                }
                self.native_values_identical(left.value, right.value, traversal)
            });
        traversal.arrays.pop();
        result
    }

    fn native_values_equal(
        &self,
        left: i64,
        right: i64,
        traversal: &mut NativeComparisonTraversal,
    ) -> Option<bool> {
        let left_encoded = left;
        let right_encoded = right;
        let left = self.native_comparison_value(left_encoded)?;
        let right = self.native_comparison_value(right_encoded)?;
        if matches!(
            left,
            NativeComparisonValue::Bool(_) | NativeComparisonValue::Null
        ) || matches!(
            right,
            NativeComparisonValue::Bool(_) | NativeComparisonValue::Null
        ) {
            return self
                .native_values_compare(left_encoded, right_encoded, traversal)
                .map(std::cmp::Ordering::is_eq);
        }
        match (left, right) {
            (
                NativeComparisonValue::Array {
                    identity: left_identity,
                    entries: left,
                },
                NativeComparisonValue::Array {
                    identity: right_identity,
                    entries: right,
                },
            ) => self.native_arrays_equal(left_identity, left, right_identity, right, traversal),
            (NativeComparisonValue::Array { .. }, _) | (_, NativeComparisonValue::Array { .. }) => {
                Some(false)
            }
            (NativeComparisonValue::Object(left), NativeComparisonValue::Object(right)) => {
                self.native_objects_equal(left, right, traversal)
            }
            (NativeComparisonValue::Object(_), _)
            | (_, NativeComparisonValue::Object(_))
            | (NativeComparisonValue::Resource(_), _)
            | (_, NativeComparisonValue::Resource(_)) => Some(matches!(
                (left, right),
                (
                    NativeComparisonValue::Resource(left),
                    NativeComparisonValue::Resource(right)
                ) if left == right
            )),
            (
                NativeComparisonValue::OpaqueIdentity(left),
                NativeComparisonValue::OpaqueIdentity(right),
            ) => Some(left == right),
            (NativeComparisonValue::OpaqueIdentity(_), _)
            | (_, NativeComparisonValue::OpaqueIdentity(_)) => Some(false),
            _ => native_comparison_values_order(left, right).map(|order| order.is_eq()),
        }
    }

    fn native_arrays_equal(
        &self,
        left_identity: usize,
        left: &[php_jit::JitNativeDirectArrayEntry],
        right_identity: usize,
        right: &[php_jit::JitNativeDirectArrayEntry],
        traversal: &mut NativeComparisonTraversal,
    ) -> Option<bool> {
        if left_identity == right_identity {
            return Some(true);
        }
        if left.len() != right.len() {
            return Some(false);
        }
        let pair = (left_identity, right_identity);
        if traversal.arrays.contains(&pair) {
            return None;
        }
        traversal.arrays.push(pair);
        let result = left.iter().try_fold(true, |equal, left| {
            if !equal {
                return Some(false);
            }
            let Some(right) = right.iter().find(|right| {
                self.native_compare_array_keys(left.key, right.key)
                    == Some(std::cmp::Ordering::Equal)
            }) else {
                return Some(false);
            };
            self.native_values_equal(left.value, right.value, traversal)
        });
        traversal.arrays.pop();
        result
    }

    fn native_objects_equal(
        &self,
        left: NativeComparisonObject<'_>,
        right: NativeComparisonObject<'_>,
        traversal: &mut NativeComparisonTraversal,
    ) -> Option<bool> {
        if left.identity == right.identity {
            return Some(true);
        }
        let (Some(left_layout), Some(right_layout)) = (left.layout_id, right.layout_id) else {
            return None;
        };
        let pair = (left.identity, right.identity);
        if traversal.objects.contains(&pair) {
            return None;
        }
        traversal.objects.push(pair);
        let result = left
            .owner
            .with_native_comparison_view(
                left_layout,
                |left_class, left_names, left_slots, left_dynamic_order, left_dynamic| {
                    right.owner.with_native_comparison_view(
                        right_layout,
                        |right_class,
                         right_names,
                         right_slots,
                         right_dynamic_order,
                         right_dynamic| {
                            if left_class != right_class {
                                return Some(false);
                            }
                            let left_count = left_slots
                                .iter()
                                .filter(|slot| slot.initialized != 0)
                                .count()
                                + left_dynamic_order
                                    .iter()
                                    .filter(|name| {
                                        left_dynamic
                                            .get(*name)
                                            .is_some_and(|cell| cell.slot.initialized != 0)
                                    })
                                    .count();
                            let right_count = right_slots
                                .iter()
                                .filter(|slot| slot.initialized != 0)
                                .count()
                                + right_dynamic_order
                                    .iter()
                                    .filter(|name| {
                                        right_dynamic
                                            .get(*name)
                                            .is_some_and(|cell| cell.slot.initialized != 0)
                                    })
                                    .count();
                            if left_count != right_count {
                                return Some(false);
                            }
                            let left_properties = left_names
                                .iter()
                                .zip(left_slots)
                                .filter(|(_, slot)| slot.initialized != 0)
                                .chain(left_dynamic_order.iter().filter_map(|name| {
                                    left_dynamic
                                        .get(name)
                                        .filter(|cell| cell.slot.initialized != 0)
                                        .map(|cell| (name, &cell.slot))
                                }));
                            let right_properties = right_names
                                .iter()
                                .zip(right_slots)
                                .filter(|(_, slot)| slot.initialized != 0)
                                .chain(right_dynamic_order.iter().filter_map(|name| {
                                    right_dynamic
                                        .get(name)
                                        .filter(|cell| cell.slot.initialized != 0)
                                        .map(|cell| (name, &cell.slot))
                                }));
                            for ((left_name, left_slot), (right_name, right_slot)) in
                                left_properties.zip(right_properties)
                            {
                                if left_name != right_name {
                                    return Some(false);
                                }
                                if !self.native_values_equal(
                                    left_slot.value,
                                    right_slot.value,
                                    traversal,
                                )? {
                                    return Some(false);
                                }
                            }
                            Some(true)
                        },
                    )
                },
            )
            .flatten()
            .flatten();
        traversal.objects.pop();
        result
    }

    fn native_values_compare(
        &self,
        left: i64,
        right: i64,
        traversal: &mut NativeComparisonTraversal,
    ) -> Option<std::cmp::Ordering> {
        let left = self.native_comparison_value(left)?;
        let right = self.native_comparison_value(right)?;
        if matches!(left, NativeComparisonValue::Bool(_))
            || matches!(right, NativeComparisonValue::Bool(_))
        {
            return Some(native_comparison_truthy(left).cmp(&native_comparison_truthy(right)));
        }
        match (left, right) {
            (NativeComparisonValue::Null, NativeComparisonValue::String(right)) => {
                return Some([].as_slice().cmp(right));
            }
            (NativeComparisonValue::String(left), NativeComparisonValue::Null) => {
                return Some(left.cmp([].as_slice()));
            }
            (NativeComparisonValue::Null, _) | (_, NativeComparisonValue::Null) => {
                return Some(native_comparison_truthy(left).cmp(&native_comparison_truthy(right)));
            }
            _ => {}
        }
        match (left, right) {
            (
                NativeComparisonValue::Array {
                    identity: left_identity,
                    entries: left,
                },
                NativeComparisonValue::Array {
                    identity: right_identity,
                    entries: right,
                },
            ) => self.native_arrays_compare(left_identity, left, right_identity, right, traversal),
            (NativeComparisonValue::Array { .. }, _) => Some(std::cmp::Ordering::Greater),
            (_, NativeComparisonValue::Array { .. }) => Some(std::cmp::Ordering::Less),
            (NativeComparisonValue::Object(left), NativeComparisonValue::Object(right)) => {
                self.native_objects_compare(left, right, traversal)
            }
            (NativeComparisonValue::Object(_), _) => Some(std::cmp::Ordering::Greater),
            (_, NativeComparisonValue::Object(_)) => Some(std::cmp::Ordering::Less),
            (
                NativeComparisonValue::OpaqueIdentity(left),
                NativeComparisonValue::OpaqueIdentity(right),
            ) if left == right => Some(std::cmp::Ordering::Equal),
            (
                NativeComparisonValue::OpaqueIdentity(_),
                NativeComparisonValue::OpaqueIdentity(_),
            ) => {
                traversal.unordered = true;
                Some(std::cmp::Ordering::Greater)
            }
            (NativeComparisonValue::OpaqueIdentity(_), _)
            | (_, NativeComparisonValue::OpaqueIdentity(_)) => None,
            (NativeComparisonValue::Resource(left), NativeComparisonValue::Resource(right)) => {
                Some(left.cmp(&right))
            }
            (NativeComparisonValue::Resource(_), _) => Some(std::cmp::Ordering::Greater),
            (_, NativeComparisonValue::Resource(_)) => Some(std::cmp::Ordering::Less),
            _ => {
                traversal.unordered |= matches!(left, NativeComparisonValue::Float(value) if value.is_nan())
                    || matches!(right, NativeComparisonValue::Float(value) if value.is_nan());
                native_comparison_values_order(left, right)
            }
        }
    }

    fn native_arrays_compare(
        &self,
        left_identity: usize,
        left: &[php_jit::JitNativeDirectArrayEntry],
        right_identity: usize,
        right: &[php_jit::JitNativeDirectArrayEntry],
        traversal: &mut NativeComparisonTraversal,
    ) -> Option<std::cmp::Ordering> {
        if left_identity == right_identity {
            return Some(std::cmp::Ordering::Equal);
        }
        match left.len().cmp(&right.len()) {
            std::cmp::Ordering::Equal => {}
            ordering => return Some(ordering),
        }
        let pair = (left_identity, right_identity);
        if traversal.arrays.contains(&pair) {
            return None;
        }
        traversal.arrays.push(pair);
        let result = left
            .iter()
            .try_fold(std::cmp::Ordering::Equal, |ordering, left| {
                if !ordering.is_eq() {
                    return Some(ordering);
                }
                let Some(right_entry) = right.iter().find(|right| {
                    self.native_compare_array_keys(left.key, right.key)
                        == Some(std::cmp::Ordering::Equal)
                }) else {
                    let right_key = right.first()?.key;
                    return self.native_compare_array_keys(left.key, right_key);
                };
                self.native_values_compare(left.value, right_entry.value, traversal)
            });
        traversal.arrays.pop();
        result
    }

    fn native_objects_compare(
        &self,
        left: NativeComparisonObject<'_>,
        right: NativeComparisonObject<'_>,
        traversal: &mut NativeComparisonTraversal,
    ) -> Option<std::cmp::Ordering> {
        if left.identity == right.identity {
            return Some(std::cmp::Ordering::Equal);
        }
        let (Some(left_layout), Some(right_layout)) = (left.layout_id, right.layout_id) else {
            return None;
        };
        let pair = (left.identity, right.identity);
        if traversal.objects.contains(&pair) {
            return None;
        }
        traversal.objects.push(pair);
        let result = left
            .owner
            .with_native_comparison_view(
                left_layout,
                |left_class, left_names, left_slots, left_dynamic_order, left_dynamic| {
                    right.owner.with_native_comparison_view(
                        right_layout,
                        |right_class,
                         right_names,
                         right_slots,
                         right_dynamic_order,
                         right_dynamic| {
                            match left_class.cmp(right_class) {
                                std::cmp::Ordering::Equal => {}
                                ordering => return Some(ordering),
                            }
                            let left_count = left_slots
                                .iter()
                                .filter(|slot| slot.initialized != 0)
                                .count()
                                + left_dynamic_order
                                    .iter()
                                    .filter(|name| {
                                        left_dynamic
                                            .get(*name)
                                            .is_some_and(|cell| cell.slot.initialized != 0)
                                    })
                                    .count();
                            let right_count = right_slots
                                .iter()
                                .filter(|slot| slot.initialized != 0)
                                .count()
                                + right_dynamic_order
                                    .iter()
                                    .filter(|name| {
                                        right_dynamic
                                            .get(*name)
                                            .is_some_and(|cell| cell.slot.initialized != 0)
                                    })
                                    .count();
                            match left_count.cmp(&right_count) {
                                std::cmp::Ordering::Equal => {}
                                ordering => return Some(ordering),
                            }
                            let left_properties = left_names
                                .iter()
                                .zip(left_slots)
                                .filter(|(_, slot)| slot.initialized != 0)
                                .chain(left_dynamic_order.iter().filter_map(|name| {
                                    left_dynamic
                                        .get(name)
                                        .filter(|cell| cell.slot.initialized != 0)
                                        .map(|cell| (name, &cell.slot))
                                }));
                            let right_properties = right_names
                                .iter()
                                .zip(right_slots)
                                .filter(|(_, slot)| slot.initialized != 0)
                                .chain(right_dynamic_order.iter().filter_map(|name| {
                                    right_dynamic
                                        .get(name)
                                        .filter(|cell| cell.slot.initialized != 0)
                                        .map(|cell| (name, &cell.slot))
                                }));
                            for ((left_name, left_slot), (right_name, right_slot)) in
                                left_properties.zip(right_properties)
                            {
                                match left_name.cmp(right_name) {
                                    std::cmp::Ordering::Equal => {}
                                    ordering => return Some(ordering),
                                }
                                match self.native_values_compare(
                                    left_slot.value,
                                    right_slot.value,
                                    traversal,
                                )? {
                                    std::cmp::Ordering::Equal => {}
                                    ordering => return Some(ordering),
                                }
                            }
                            Some(std::cmp::Ordering::Equal)
                        },
                    )
                },
            )
            .flatten()
            .flatten();
        traversal.objects.pop();
        result
    }

    fn native_compare_array_keys(&self, left: i64, right: i64) -> Option<std::cmp::Ordering> {
        match (
            self.native_comparison_value(left)?,
            self.native_comparison_value(right)?,
        ) {
            (NativeComparisonValue::Int(left), NativeComparisonValue::Int(right)) => {
                Some(left.cmp(&right))
            }
            (NativeComparisonValue::String(left), NativeComparisonValue::String(right)) => {
                Some(left.cmp(right))
            }
            (NativeComparisonValue::Int(_), NativeComparisonValue::String(_)) => {
                Some(std::cmp::Ordering::Less)
            }
            (NativeComparisonValue::String(_), NativeComparisonValue::Int(_)) => {
                Some(std::cmp::Ordering::Greater)
            }
            _ => None,
        }
    }

    fn native_print_r_starts_multiline(&self, encoded: i64) -> Option<bool> {
        Some(matches!(
            self.native_comparison_value(encoded)?,
            NativeComparisonValue::Array { .. } | NativeComparisonValue::Object(_)
        ))
    }

    fn native_mysqli_connection_id(&self, encoded: i64) -> Option<i64> {
        let NativeComparisonValue::Object(object) = self.native_comparison_value(encoded)? else {
            return None;
        };
        if !object.owner.display_name().eq_ignore_ascii_case("mysqli") {
            return None;
        }
        let layout = object.layout_id?;
        let encoded = object
            .owner
            .with_native_array_cast_view(
                layout,
                |declared_names, declared, _dynamic_order, dynamic| {
                    declared_names
                        .iter()
                        .zip(declared)
                        .find_map(|(name, slot)| {
                            (name == "__mysqli_connection" && slot.initialized != 0)
                                .then_some(slot.value)
                        })
                        .or_else(|| {
                            dynamic
                                .get("__mysqli_connection")
                                .filter(|cell| cell.slot.initialized != 0)
                                .map(|cell| cell.slot.value)
                        })
                },
            )
            .flatten()?;
        match self.native_comparison_value(encoded)? {
            NativeComparisonValue::Int(id) => Some(id),
            _ => None,
        }
    }

    fn native_mysqli_object(&self, encoded: i64) -> Option<()> {
        let NativeComparisonValue::Object(object) = self.native_comparison_value(encoded)? else {
            return None;
        };
        object
            .owner
            .display_name()
            .eq_ignore_ascii_case("mysqli")
            .then_some(())
    }

    fn store_native_mysqli_property_owned(
        &mut self,
        encoded: i64,
        property: &str,
        value: i64,
    ) -> Option<()> {
        self.native_mysqli_object(encoded)?;
        let slot = self.exact_named_dynamic_property_slot_location(encoded, property)?;
        // Safety: this exact call owns the replacement and has exclusive
        // access to the request-owned slot for the synchronous activation.
        #[allow(unsafe_code)]
        let previous = unsafe {
            let previous = ((*slot).initialized != 0).then_some((*slot).value);
            (*slot).value = value;
            (*slot).initialized = 1;
            previous
        };
        if let Some(previous) = previous {
            self.discard_owned_direct_value(previous).ok()?;
        }
        Some(())
    }

    fn native_mysqli_result_id(&self, encoded: i64) -> Option<i64> {
        let NativeComparisonValue::Object(object) = self.native_comparison_value(encoded)? else {
            return None;
        };
        if !object
            .owner
            .display_name()
            .eq_ignore_ascii_case("mysqli_result")
        {
            return None;
        }
        let layout = object.layout_id?;
        let encoded = object
            .owner
            .with_native_array_cast_view(
                layout,
                |declared_names, declared, _dynamic_order, dynamic| {
                    declared_names
                        .iter()
                        .zip(declared)
                        .find_map(|(name, slot)| {
                            (name == "__mysqli_result" && slot.initialized != 0)
                                .then_some(slot.value)
                        })
                        .or_else(|| {
                            dynamic
                                .get("__mysqli_result")
                                .filter(|cell| cell.slot.initialized != 0)
                                .map(|cell| cell.slot.value)
                        })
                },
            )
            .flatten()?;
        match self.native_comparison_value(encoded)? {
            NativeComparisonValue::Int(id) => Some(id),
            _ => None,
        }
    }

    fn native_mysqli_invalidate_result(&mut self, encoded: i64) -> Option<()> {
        let slot = self.exact_named_dynamic_property_slot_location(encoded, "__mysqli_result")?;
        // Safety: the slot belongs to the request-owned direct object and the
        // exact call executes synchronously with exclusive fast-state access.
        #[allow(unsafe_code)]
        unsafe {
            (*slot).value = php_jit::jit_encode_constant(u32::MAX);
            (*slot).initialized = 0;
        }
        Some(())
    }

    fn native_mysqli_invalidate_connection(&mut self, encoded: i64) -> Option<()> {
        let slot =
            self.exact_named_dynamic_property_slot_location(encoded, "__mysqli_connection")?;
        // Safety: the slot belongs to the request-owned direct object and the
        // exact call executes synchronously with exclusive fast-state access.
        #[allow(unsafe_code)]
        unsafe {
            (*slot).value = php_jit::jit_encode_constant(u32::MAX);
            (*slot).initialized = 0;
        }
        Some(())
    }

    fn write_native_export_string(output: &mut Vec<u8>, bytes: &[u8]) {
        output.push(b'\'');
        for byte in bytes {
            match byte {
                0 => output.extend_from_slice(b"' . \"\\0\" . '"),
                b'\\' => output.extend_from_slice(b"\\\\"),
                b'\'' => output.extend_from_slice(b"\\'"),
                byte => output.push(*byte),
            }
        }
        output.push(b'\'');
    }

    fn native_var_export_starts_multiline(
        &self,
        encoded: i64,
        traversal: &NativeJsonTraversal,
    ) -> Option<bool> {
        match self.native_comparison_value(encoded)? {
            NativeComparisonValue::Array { identity, .. } => {
                Some(!traversal.array_is_active(identity))
            }
            NativeComparisonValue::Object(object) => {
                Some(!traversal.object_is_active(object.identity))
            }
            _ => Some(false),
        }
    }

    fn write_native_var_export(
        &self,
        encoded: i64,
        indent: usize,
        output: &mut Vec<u8>,
        traversal: &mut NativeJsonTraversal,
    ) -> Option<()> {
        let write_indent = |output: &mut Vec<u8>, width: usize| {
            output.extend(std::iter::repeat_n(b' ', width));
        };
        match self.native_comparison_value(encoded)? {
            NativeComparisonValue::Null => output.extend_from_slice(b"NULL"),
            NativeComparisonValue::Bool(true) => output.extend_from_slice(b"true"),
            NativeComparisonValue::Bool(false) => output.extend_from_slice(b"false"),
            NativeComparisonValue::Int(value) => {
                let mut bytes = [0_u8; 20];
                output.extend_from_slice(native_i64_ascii(value, &mut bytes));
            }
            NativeComparisonValue::Float(value) => {
                let precision = self.native_serialize_precision()?;
                output.extend_from_slice(
                    php_runtime::api::php_float_export_string(
                        php_runtime::api::FloatValue::from_f64(value),
                        precision,
                    )
                    .as_bytes(),
                );
            }
            NativeComparisonValue::String(bytes) => {
                Self::write_native_export_string(output, bytes);
            }
            NativeComparisonValue::Array { identity, entries } => {
                if traversal.array_is_active(identity) {
                    return None;
                }
                traversal.push_array(identity)?;
                output.extend_from_slice(b"array (\n");
                for entry in entries {
                    write_indent(output, indent + 2);
                    match self.native_comparison_value(entry.key)? {
                        NativeComparisonValue::Int(key) => {
                            let mut bytes = [0_u8; 20];
                            output.extend_from_slice(native_i64_ascii(key, &mut bytes));
                        }
                        NativeComparisonValue::String(key) => {
                            Self::write_native_export_string(output, key);
                        }
                        _ => return None,
                    }
                    output.extend_from_slice(b" => ");
                    if self.native_var_export_starts_multiline(entry.value, traversal)? {
                        output.push(b'\n');
                        write_indent(output, indent + 2);
                    }
                    self.write_native_var_export(entry.value, indent + 2, output, traversal)?;
                    output.extend_from_slice(b",\n");
                }
                write_indent(output, indent);
                output.push(b')');
                traversal.pop_array();
            }
            NativeComparisonValue::Object(object) => {
                if traversal.object_is_active(object.identity) {
                    return None;
                }
                let layout = object.layout_id?;
                traversal.push_object(object.identity)?;
                let std_class = object.owner.display_name().eq_ignore_ascii_case("stdClass");
                if std_class {
                    output.extend_from_slice(b"(object) array(\n");
                } else {
                    output.push(b'\\');
                    output.extend_from_slice(object.owner.display_name().as_bytes());
                    output.extend_from_slice(b"::__set_state(array(\n");
                }
                let result = object
                    .owner
                    .with_native_array_cast_view(
                        layout,
                        |declared_names, declared, dynamic_order, dynamic| {
                            for (name, slot) in declared_names.iter().zip(declared) {
                                if slot.initialized == 0 || name == "__phrust_trace_string" {
                                    continue;
                                }
                                write_indent(output, indent + 3);
                                Self::write_native_export_string(output, name.as_bytes());
                                output.extend_from_slice(b" => ");
                                if self.native_var_export_starts_multiline(
                                    slot.value,
                                    traversal,
                                )? {
                                    output.push(b'\n');
                                    write_indent(output, indent + 2);
                                }
                                self.write_native_var_export(
                                    slot.value,
                                    indent + 2,
                                    output,
                                    traversal,
                                )?;
                                output.extend_from_slice(b",\n");
                            }
                            for name in dynamic_order {
                                let property = dynamic.get(name)?;
                                if property.slot.initialized == 0
                                    || name.as_str() == "__phrust_trace_string"
                                {
                                    continue;
                                }
                                write_indent(output, indent + 3);
                                Self::write_native_export_string(output, name.as_bytes());
                                output.extend_from_slice(b" => ");
                                if self.native_var_export_starts_multiline(
                                    property.slot.value,
                                    traversal,
                                )? {
                                    output.push(b'\n');
                                    write_indent(output, indent + 2);
                                }
                                self.write_native_var_export(
                                    property.slot.value,
                                    indent + 2,
                                    output,
                                    traversal,
                                )?;
                                output.extend_from_slice(b",\n");
                            }
                            write_indent(output, indent);
                            output.extend_from_slice(if std_class { b")" } else { b"))" });
                            Some(())
                        },
                    )
                    .flatten();
                traversal.pop_object();
                result?;
            }
            NativeComparisonValue::Resource(identity) => {
                output.extend_from_slice(b"NULL /* resource #");
                let mut bytes = [0_u8; 20];
                output.extend_from_slice(native_i64_ascii(
                    i64::try_from(identity).ok()?,
                    &mut bytes,
                ));
                output.extend_from_slice(b" */");
            }
            NativeComparisonValue::OpaqueIdentity(_) => return None,
        }
        Some(())
    }

    fn write_native_print_r(
        &self,
        encoded: i64,
        indent: usize,
        output: &mut Vec<u8>,
        traversal: &mut NativeJsonTraversal,
    ) -> Option<()> {
        let write_indent = |output: &mut Vec<u8>, width: usize| {
            output.extend(std::iter::repeat_n(b' ', width));
        };
        match self.native_comparison_value(encoded)? {
            NativeComparisonValue::Null | NativeComparisonValue::Bool(false) => {}
            NativeComparisonValue::Bool(true) => output.push(b'1'),
            NativeComparisonValue::Int(value) => {
                let mut bytes = [0_u8; 20];
                output.extend_from_slice(native_i64_ascii(value, &mut bytes));
            }
            NativeComparisonValue::Float(value) => {
                let mut bytes = [0_u8; php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY];
                output.extend_from_slice(php_runtime::api::float_to_php_string_bytes(
                    value, &mut bytes,
                ));
            }
            NativeComparisonValue::String(bytes) => output.extend_from_slice(bytes),
            NativeComparisonValue::Array { identity, entries } => {
                if traversal.array_is_active(identity) {
                    output.extend_from_slice(b"Array\n *RECURSION*");
                    return Some(());
                }
                traversal.push_array(identity)?;
                output.extend_from_slice(b"Array\n");
                write_indent(output, indent);
                output.extend_from_slice(b"(\n");
                for entry in entries {
                    write_indent(output, indent + 4);
                    output.push(b'[');
                    match self.native_comparison_value(entry.key)? {
                        NativeComparisonValue::Int(key) => {
                            let mut bytes = [0_u8; 20];
                            output.extend_from_slice(native_i64_ascii(key, &mut bytes));
                        }
                        NativeComparisonValue::String(key) => output.extend_from_slice(key),
                        _ => return None,
                    }
                    output.extend_from_slice(b"] => ");
                    let child_indent = if self.native_print_r_starts_multiline(entry.value)? {
                        indent + 8
                    } else {
                        indent + 4
                    };
                    self.write_native_print_r(entry.value, child_indent, output, traversal)?;
                    output.push(b'\n');
                }
                write_indent(output, indent);
                output.extend_from_slice(b")\n");
                traversal.pop_array();
            }
            NativeComparisonValue::Object(object) => {
                if traversal.object_is_active(object.identity) {
                    output.extend_from_slice(b"*RECURSION*");
                    return Some(());
                }
                let layout = object.layout_id?;
                traversal.push_object(object.identity)?;
                output.extend_from_slice(object.owner.display_name().as_bytes());
                output.extend_from_slice(b" Object\n");
                write_indent(output, indent);
                output.extend_from_slice(b"(\n");
                let result = object
                    .owner
                    .with_native_array_cast_view(
                        layout,
                        |declared_names, declared, dynamic_order, dynamic| {
                            for (name, slot) in declared_names.iter().zip(declared) {
                                if slot.initialized == 0 {
                                    continue;
                                }
                                write_indent(output, indent + 4);
                                output.push(b'[');
                                let bytes = name.as_bytes();
                                if let Some(rest) = bytes.strip_prefix(b"\0*\0") {
                                    output.extend_from_slice(rest);
                                    output.extend_from_slice(b":protected");
                                } else if let Some(rest) = bytes.strip_prefix(b"\0") {
                                    let split = rest.iter().position(|byte| *byte == 0)?;
                                    output.extend_from_slice(&rest[split + 1..]);
                                    output.push(b':');
                                    output.extend_from_slice(&rest[..split]);
                                    output.extend_from_slice(b":private");
                                } else {
                                    output.extend_from_slice(bytes);
                                }
                                output.extend_from_slice(b"] => ");
                                let child_indent =
                                    if self.native_print_r_starts_multiline(slot.value)? {
                                        indent + 8
                                    } else {
                                        indent + 4
                                    };
                                self.write_native_print_r(
                                    slot.value,
                                    child_indent,
                                    output,
                                    traversal,
                                )?;
                                output.push(b'\n');
                            }
                            for name in dynamic_order {
                                let property = dynamic.get(name)?;
                                if property.slot.initialized == 0 {
                                    continue;
                                }
                                write_indent(output, indent + 4);
                                output.push(b'[');
                                output.extend_from_slice(name.as_bytes());
                                output.extend_from_slice(b"] => ");
                                let child_indent =
                                    if self.native_print_r_starts_multiline(property.slot.value)? {
                                        indent + 8
                                    } else {
                                        indent + 4
                                    };
                                self.write_native_print_r(
                                    property.slot.value,
                                    child_indent,
                                    output,
                                    traversal,
                                )?;
                                output.push(b'\n');
                            }
                            write_indent(output, indent);
                            output.extend_from_slice(b")\n");
                            Some(())
                        },
                    )
                    .flatten();
                traversal.pop_object();
                result?;
            }
            NativeComparisonValue::Resource(identity) => {
                output.extend_from_slice(b"Resource id #");
                let mut bytes = [0_u8; 20];
                output
                    .extend_from_slice(native_i64_ascii(i64::try_from(identity).ok()?, &mut bytes));
            }
            NativeComparisonValue::OpaqueIdentity(_) => {
                output.extend_from_slice(b"Closure Object\n(\n)\n")
            }
        }
        Some(())
    }

    fn write_native_var_dump(
        &self,
        encoded: i64,
        indent: usize,
        output: &mut Vec<u8>,
        traversal: &mut NativeJsonTraversal,
    ) -> Option<()> {
        let write_indent = |output: &mut Vec<u8>, width: usize| {
            output.extend(std::iter::repeat_n(b' ', width));
        };
        match self.native_comparison_value(encoded)? {
            NativeComparisonValue::Null => output.extend_from_slice(b"NULL\n"),
            NativeComparisonValue::Bool(value) => {
                output.extend_from_slice(if value {
                    b"bool(true)\n"
                } else {
                    b"bool(false)\n"
                });
            }
            NativeComparisonValue::Int(value) => {
                output.extend_from_slice(b"int(");
                let mut bytes = [0_u8; 20];
                output.extend_from_slice(native_i64_ascii(value, &mut bytes));
                output.extend_from_slice(b")\n");
            }
            NativeComparisonValue::Float(value) => {
                output.extend_from_slice(b"float(");
                let mut bytes = [0_u8; php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY];
                output.extend_from_slice(php_runtime::api::float_to_php_string_bytes(
                    value, &mut bytes,
                ));
                output.extend_from_slice(b")\n");
            }
            NativeComparisonValue::String(bytes) => {
                output.extend_from_slice(b"string(");
                let mut length = [0_u8; 20];
                output.extend_from_slice(native_i64_ascii(
                    i64::try_from(bytes.len()).ok()?,
                    &mut length,
                ));
                output.extend_from_slice(b") \"");
                output.extend_from_slice(bytes);
                output.extend_from_slice(b"\"\n");
            }
            NativeComparisonValue::Array { identity, entries } => {
                if traversal.array_is_active(identity) {
                    output.extend_from_slice(b"*RECURSION*\n");
                    return Some(());
                }
                traversal.push_array(identity)?;
                output.extend_from_slice(b"array(");
                let mut length = [0_u8; 20];
                output.extend_from_slice(native_i64_ascii(
                    i64::try_from(entries.len()).ok()?,
                    &mut length,
                ));
                output.extend_from_slice(b") {\n");
                for entry in entries {
                    write_indent(output, indent + 2);
                    output.push(b'[');
                    match self.native_comparison_value(entry.key)? {
                        NativeComparisonValue::Int(key) => {
                            let mut bytes = [0_u8; 20];
                            output.extend_from_slice(native_i64_ascii(key, &mut bytes));
                        }
                        NativeComparisonValue::String(key) => {
                            output.push(b'\"');
                            output.extend_from_slice(key);
                            output.push(b'\"');
                        }
                        _ => return None,
                    }
                    output.extend_from_slice(b"]=>\n");
                    write_indent(output, indent + 2);
                    self.write_native_var_dump(entry.value, indent + 2, output, traversal)?;
                }
                write_indent(output, indent);
                output.extend_from_slice(b"}\n");
                traversal.pop_array();
            }
            NativeComparisonValue::Object(object) => {
                if traversal.object_is_active(object.identity) {
                    output.extend_from_slice(b"*RECURSION*\n");
                    return Some(());
                }
                let layout = object.layout_id?;
                traversal.push_object(object.identity)?;
                let result = object
                    .owner
                    .with_native_array_cast_view(
                        layout,
                        |declared_names, declared, dynamic_order, dynamic| {
                            let count =
                                declared.iter().filter(|slot| slot.initialized != 0).count()
                                    + dynamic_order
                                        .iter()
                                        .filter(|name| {
                                            dynamic
                                                .get(*name)
                                                .is_some_and(|cell| cell.slot.initialized != 0)
                                        })
                                        .count();
                            output.extend_from_slice(b"object(");
                            output.extend_from_slice(object.owner.display_name().as_bytes());
                            output.extend_from_slice(b")#");
                            let mut identity = [0_u8; 20];
                            output.extend_from_slice(native_i64_ascii(
                                i64::try_from(object.identity).ok()?,
                                &mut identity,
                            ));
                            output.extend_from_slice(b" (");
                            let mut count_bytes = [0_u8; 20];
                            output.extend_from_slice(native_i64_ascii(
                                i64::try_from(count).ok()?,
                                &mut count_bytes,
                            ));
                            output.extend_from_slice(b") {\n");
                            for (name, slot) in declared_names.iter().zip(declared) {
                                if slot.initialized == 0 {
                                    continue;
                                }
                                write_indent(output, indent + 2);
                                output.extend_from_slice(b"[\"");
                                output.extend_from_slice(name.as_bytes());
                                output.extend_from_slice(b"\"]=>\n");
                                write_indent(output, indent + 2);
                                self.write_native_var_dump(
                                    slot.value,
                                    indent + 2,
                                    output,
                                    traversal,
                                )?;
                            }
                            for name in dynamic_order {
                                let property = dynamic.get(name)?;
                                if property.slot.initialized == 0 {
                                    continue;
                                }
                                write_indent(output, indent + 2);
                                output.extend_from_slice(b"[\"");
                                output.extend_from_slice(name.as_bytes());
                                output.extend_from_slice(b"\"]=>\n");
                                write_indent(output, indent + 2);
                                self.write_native_var_dump(
                                    property.slot.value,
                                    indent + 2,
                                    output,
                                    traversal,
                                )?;
                            }
                            write_indent(output, indent);
                            output.extend_from_slice(b"}\n");
                            Some(())
                        },
                    )
                    .flatten();
                traversal.pop_object();
                result?;
            }
            NativeComparisonValue::Resource(identity) => {
                output.extend_from_slice(b"resource(");
                let mut bytes = [0_u8; 20];
                output
                    .extend_from_slice(native_i64_ascii(i64::try_from(identity).ok()?, &mut bytes));
                output.extend_from_slice(b") of type (Unknown)\n");
            }
            NativeComparisonValue::OpaqueIdentity(_) => output.extend_from_slice(b"object\n"),
        }
        Some(())
    }

    /// Borrows the authoritative request-local output stack without
    /// recovering the baseline coordinator.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn native_output_buffer(&mut self) -> Option<&mut php_runtime::api::OutputBuffer> {
        unsafe { self.output.as_mut() }
    }

    /// Publishes the active output-buffer bytes directly into the
    /// authoritative native string arena. Callers apply any clean/flush stack
    /// effect only after this immutable copy completes.
    fn publish_current_output_buffer(&mut self) -> Result<Option<i64>, &'static str> {
        let Some(bytes) = self
            .native_output_buffer()
            .and_then(|output| output.current_buffer_bytes())
        else {
            return Ok(None);
        };
        let source = (bytes.as_ptr(), bytes.len());
        self.publish_direct_string_with(source.1, |output| {
            if source.1 == 0 {
                return;
            }
            // SAFETY: the request-owned output stack is immutable for this
            // synchronous copy and native arena reservation cannot relocate
            // its byte range.
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::copy_nonoverlapping(source.0, output.as_mut_ptr(), source.1);
            }
        })
        .map(Some)
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn write_output_slice(&self, bytes: &[u8]) -> Result<(), &'static str> {
        let output = unsafe { self.output.as_mut() }.ok_or("native output is unavailable")?;
        output.write_bytes(bytes);
        Ok(())
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    pub(crate) fn retain_direct_encoded(&mut self, encoded: i64) -> Result<(), &'static str> {
        let Some(runtime_index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        let Some(index) = runtime_index.checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
        else {
            return Err("prepared value belongs to the cold value plane");
        };
        if index as usize >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
            return Err("prepared direct value index is outside its arena");
        }
        let slots = self.header.active_runtime_view().direct_value_slots as usize
            as *mut php_jit::JitNativeValueSlot;
        // SAFETY: the encoded owner was published in this request's stable
        // direct arena and remains live through the source/template owner.
        let slot = unsafe { &mut *slots.add(index as usize) };
        if slot.refcount == 0 {
            return Err("prepared direct value owner is no longer live");
        }
        slot.refcount = slot
            .refcount
            .checked_add(1)
            .ok_or("prepared direct value refcount overflow")?;
        Ok(())
    }

    /// Publishes a new scalar/string/array constant without recovering the cold
    /// coordinator or constructing a Rust `Value`. The native map owns one
    /// handle and every prepared FetchConst slot owns one additional handle.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn publish_native_dynamic_constant(&mut self, name: String, encoded: i64) -> bool {
        if php_jit::jit_decode_runtime_value(encoded).is_some() {
            let Some((_, slot)) = self.direct_slot(encoded) else {
                return false;
            };
            if !matches!(
                slot.kind,
                php_jit::JIT_NATIVE_VALUE_VIEW_STRING
                    | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
                    | php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT
            ) {
                return false;
            }
        } else if php_jit::jit_decode_constant(encoded)
            .is_some_and(|constant| constant < php_jit::JIT_VALUE_TRUE)
        {
            // Active-unit literal handles are not stable storage. Lowering
            // must replace them with their trusted direct literal owner.
            return false;
        }

        let view = self.header.active_runtime_view();
        let plan_groups = self
            .symbol_query
            .dynamic_constant_site_groups(&name, view.trusted_constant_slots);
        let plan_count = plan_groups
            .iter()
            .fold(0_usize, |count, (_, _, group_count)| {
                count.saturating_add(*group_count)
            });

        let owner_count = 1_usize.saturating_add(plan_count);
        let mut retained = 0_usize;
        for _ in 0..owner_count {
            if self.retain_direct_encoded(encoded).is_err() {
                for _ in 0..retained {
                    self.rollback_direct_retain(encoded);
                }
                return false;
            }
            retained += 1;
        }

        let Some(constants) = (unsafe { self.symbol_query.native_dynamic_constants.as_mut() })
        else {
            for _ in 0..retained {
                self.rollback_direct_retain(encoded);
            }
            return false;
        };
        if constants.insert(name, encoded).is_some() {
            for _ in 0..retained {
                self.rollback_direct_retain(encoded);
            }
            return false;
        }
        for (slots, plan_indices, group_count) in plan_groups {
            let plans = slots as usize as *mut php_jit::JitNativeTrustedConstantSlot;
            for plan_index in 0..group_count {
                let index = unsafe { *plan_indices.add(plan_index) };
                unsafe {
                    *plans.add(index) = php_jit::JitNativeTrustedConstantSlot {
                        value: encoded,
                        state: php_jit::JIT_NATIVE_TRUSTED_CONSTANT_PUBLISHED,
                        reserved: 0,
                    };
                }
            }
        }
        let pending = view.root_mutation_pending as usize as *mut u32;
        if !pending.is_null() {
            unsafe { *pending = 1 };
        }
        true
    }

    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn rollback_direct_retain(&mut self, encoded: i64) {
        let Some(runtime_index) = php_jit::jit_decode_runtime_value(encoded) else {
            return;
        };
        let Some(index) = runtime_index.checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
        else {
            return;
        };
        let slots = self.header.active_runtime_view().direct_value_slots as usize
            as *mut php_jit::JitNativeValueSlot;
        // SAFETY: called only for a successful retain above; the preceding
        // owner remains live, so rollback cannot reach zero.
        let slot = unsafe { &mut *slots.add(index as usize) };
        debug_assert!(slot.refcount > 1);
        slot.refcount -= 1;
    }

    fn direct_owner_is_fast_discardable(&self, encoded: i64) -> bool {
        self.direct_owner_is_fast_discardable_at(encoded, 0)
    }

    #[allow(unsafe_code)]
    fn direct_owner_is_fast_discardable_at(&self, encoded: i64, depth: usize) -> bool {
        if depth > 64 {
            return false;
        }
        let Some((_, slot)) = self.direct_slot(encoded) else {
            return php_jit::jit_decode_runtime_value(encoded).is_none();
        };
        if slot.refcount > 1 {
            return true;
        }
        match slot.kind {
            php_jit::JIT_NATIVE_VALUE_VIEW_STRING
            | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
            | php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => true,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY => self
                .native_direct_array_entries(encoded)
                .is_some_and(|entries| {
                    entries.iter().all(|entry| {
                        self.direct_owner_is_fast_discardable_at(entry.key, depth + 1)
                            && self.direct_owner_is_fast_discardable_at(entry.value, depth + 1)
                    })
                }),
            php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
                if slot.flags == php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION
                    && slot.aux != 0 =>
            {
                let owner = unsafe { &*(slot.aux as usize as *const NativePreparedCallableOwner) };
                let view = owner.native_view;
                if view.kind == php_jit::JIT_NATIVE_CALLABLE_KIND_CLOSURE {
                    let implicit_this =
                        view.flags & php_jit::JIT_NATIVE_PREPARED_CLOSURE_HAS_IMPLICIT_THIS != 0;
                    if implicit_this
                        && !self.direct_owner_is_fast_discardable_at(view.implicit_this, depth + 1)
                    {
                        return false;
                    }
                    if view.capture_count == 0 {
                        return true;
                    }
                    if view.captures == 0 {
                        return false;
                    }
                    let captures = unsafe {
                        std::slice::from_raw_parts(
                            view.captures as usize as *const i64,
                            view.capture_count as usize,
                        )
                    };
                    captures
                        .iter()
                        .copied()
                        .all(|capture| self.direct_owner_is_fast_discardable_at(capture, depth + 1))
                } else if view.kind == php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_OBJECT_METHOD {
                    self.direct_owner_is_fast_discardable_at(view.receiver, depth + 1)
                } else {
                    true
                }
            }
            _ => false,
        }
    }

    /// Releases exactly one transferred owner without recovering the
    /// baseline coordinator. Exact structured results contain only native
    /// scalars, strings, and acyclic arrays; retiring their final owner can
    /// therefore reclaim the stable arenas and recursively release children
    /// entirely inside the authoritative direct plane.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn discard_owned_direct_value(&mut self, encoded: i64) -> Result<(), &'static str> {
        let Some(runtime_index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        let Some(index) = runtime_index.checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
        else {
            return Err("transferred value belongs to the cold value plane");
        };
        let index = index as usize;
        if index >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
            return Err("transferred direct value index is outside its stable arena");
        }
        let view = self.header.active_runtime_view();
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        let slot = unsafe { *slots.add(index) };
        if slot.refcount == 0 {
            return Err("transferred direct value owner is no longer live");
        }
        if slot.refcount > 1 {
            unsafe {
                (*slots.add(index)).refcount -= 1;
            }
            return Ok(());
        }

        let mut array_children = None;
        let mut callable_children = None;
        match slot.kind {
            php_jit::JIT_NATIVE_VALUE_VIEW_STRING
                if slot.flags == php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION =>
            {
                let base = view.direct_string_bytes as usize;
                let start = (slot.aux as usize)
                    .checked_sub(base)
                    .ok_or("direct string owner is outside its stable arena")?;
                self.free_direct_string_range(
                    start,
                    php_jit::jit_native_direct_string_capacity(slot.reserved),
                );
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
                if slot.flags == php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION => {}
            php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => {}
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY => {
                let length = usize::try_from(slot.payload)
                    .map_err(|_| "direct array length overflow during owner discard")?;
                let base = view.direct_array_entries as usize;
                let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
                let offset = (slot.aux as usize)
                    .checked_sub(base)
                    .ok_or("direct array owner is outside its stable arena")?;
                if !offset.is_multiple_of(entry_size) {
                    return Err("direct array owner is not entry-aligned");
                }
                let start = offset / entry_size;
                if start
                    .checked_add(length)
                    .is_none_or(|end| end > php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY)
                {
                    return Err("direct array owner length is outside its stable arena");
                }
                let entries =
                    view.direct_array_entries as usize as *const php_jit::JitNativeDirectArrayEntry;
                array_children = Some((unsafe { entries.add(start) }, length));
                self.free_direct_array_range(start, slot.reserved);
                let states =
                    view.direct_array_states as usize as *mut php_jit::JitNativeDirectArrayState;
                unsafe {
                    *states.add(index) = php_jit::JitNativeDirectArrayState::default();
                }
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
                if slot.flags == php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION
                    && slot.aux != 0 =>
            {
                let callable =
                    unsafe { Box::from_raw(slot.aux as usize as *mut NativePreparedCallableOwner) };
                let callable_view = callable.native_view;
                let mut children = smallvec::SmallVec::<[i64; 8]>::new();
                if callable_view.kind == php_jit::JIT_NATIVE_CALLABLE_KIND_CLOSURE {
                    if !self.direct_closure_handles.is_null() {
                        unsafe {
                            (*self.direct_closure_handles).remove(&slot.payload);
                        }
                    }
                    if callable_view.flags & php_jit::JIT_NATIVE_PREPARED_CLOSURE_HAS_IMPLICIT_THIS
                        != 0
                    {
                        children.push(callable_view.implicit_this);
                    }
                    if callable_view.capture_count != 0 {
                        debug_assert_ne!(callable_view.captures, 0);
                        if callable_view.captures != 0 {
                            let captures = unsafe {
                                std::slice::from_raw_parts(
                                    callable_view.captures as usize as *const i64,
                                    callable_view.capture_count as usize,
                                )
                            };
                            children.extend_from_slice(captures);
                        }
                    }
                } else if callable_view.kind
                    == php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_OBJECT_METHOD
                {
                    children.push(callable_view.receiver);
                }
                callable_children = Some(children);
            }
            _ => {
                return Err(
                    "exact owner discard requires a scalar, string, or acyclic direct array",
                );
            }
        }

        let free_head = view.direct_value_free_head as usize as *mut u32;
        unsafe {
            *slots.add(index) = php_jit::JitNativeValueSlot {
                payload: u64::from(*free_head),
                ..php_jit::JitNativeValueSlot::default()
            };
            *free_head = u32::try_from(index).unwrap_or(php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE);
        }
        if let Some((entries, length)) = array_children {
            // The freed stable range is not reused during owner retirement:
            // this walk only releases children and never publishes. Copy one
            // entry before recursing so a nested array can release its own
            // disjoint range without borrowing `self` or building a Rust
            // collection of the complete child graph.
            for child_index in (0..length).rev() {
                let entry = unsafe { *entries.add(child_index) };
                self.discard_owned_direct_value(entry.value)?;
                self.discard_owned_direct_value(entry.key)?;
            }
        }
        if let Some(children) = callable_children {
            for child in children.into_iter().rev() {
                self.discard_owned_direct_value(child)?;
            }
        }
        Ok(())
    }

    /// Publishes a freshly created object into the authoritative direct plane.
    /// Any already-installed native property slots are authoritative
    /// immediately; the descriptor publishes the fixed declared-slot base,
    /// while the object owner keeps the dynamic native map.
    #[allow(unsafe_code)] // Safety: the native request owns every published pointer for the synchronous activation.
    fn publish_direct_object(
        &mut self,
        object: php_runtime::api::ObjectRef,
    ) -> Result<i64, &'static str> {
        let index = self.reserve_direct_value_index()?;
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
        let layout_id = object.class_layout_epoch();
        let native_slots = object.native_declared_slots_view(layout_id);
        let object_id = object.id();
        let object_type_flags = (u32::from(object.is_native_countable())
            * php_jit::JIT_NATIVE_OBJECT_COUNTABLE)
            | (u32::from(object.is_native_traversable()) * php_jit::JIT_NATIVE_OBJECT_TRAVERSABLE)
            | (u32::from(object.class_name().eq_ignore_ascii_case("stdClass"))
                * php_jit::JIT_NATIVE_OBJECT_STDCLASS)
            | (u32::from(object.allows_native_dynamic_properties())
                * php_jit::JIT_NATIVE_OBJECT_ALLOWS_DYNAMIC_PROPERTIES);
        let owner = Box::into_raw(Box::new(object));
        let view = self.header.active_runtime_view();
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        let owners = view.direct_object_owners as usize as *mut u64;
        let (flags, reserved, payload, aux) =
            native_slots.map_or((0, 0, object_id, 0), |(base, count)| {
                (
                    php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION | object_type_flags,
                    u32::try_from(count).unwrap_or(u32::MAX),
                    layout_id,
                    base as usize as u64,
                )
            });
        unsafe {
            *owners.add(index as usize) = owner as usize as u64;
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                refcount: 1,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
                flags,
                reserved,
                payload,
                aux,
            };
        }
        Ok((php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG | u64::from(runtime_index)) as i64)
    }
}
