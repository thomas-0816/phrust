//! Pcre builtin registry slice.

use super::core::*;
use crate::builtins::context::{PcreBuiltinServices, PcreCallbackServices, PcreServiceAccess};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinRegistry,
    BuiltinResult, RuntimeSourceSpan,
};
use crate::{CallableValue, PhpArray, PhpString, Value, pcre};
use pcre2::bytes::MatchOptions;
use std::sync::Arc;

type PregReplaceSpec = (Arc<pcre::CompiledPattern>, Vec<u8>);

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("preg_filter", exact_preg_filter, BuiltinCompatibility::Php),
    BuiltinEntry::new("preg_grep", exact_preg_grep, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "preg_last_error",
        exact_preg_last_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "preg_last_error_msg",
        exact_preg_last_error_msg,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("preg_match", exact_preg_match, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "preg_match_all",
        exact_preg_match_all,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("preg_quote", exact_preg_quote, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "preg_replace",
        exact_preg_replace,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "preg_replace_callback",
        builtin_preg_replace_callback,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "preg_replace_callback_array",
        builtin_preg_replace_callback_array,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("preg_split", exact_preg_split, BuiltinCompatibility::Php),
];

macro_rules! exact_pcre_builtin {
    ($entry:ident => $implementation:ident) => {
        #[doc(hidden)]
        pub fn $entry(
            context: &mut BuiltinContext<'_>,
            args: Vec<Value>,
            span: RuntimeSourceSpan,
        ) -> BuiltinResult {
            let mut services = context.pcre_services();
            $implementation(&mut services, args, span)
        }
    };
}

exact_pcre_builtin!(exact_preg_match => preg_match);
exact_pcre_builtin!(exact_preg_match_all => preg_match_all);
exact_pcre_builtin!(exact_preg_replace => preg_replace);
exact_pcre_builtin!(exact_preg_filter => preg_filter);
exact_pcre_builtin!(exact_preg_split => preg_split);
exact_pcre_builtin!(exact_preg_grep => preg_grep);
exact_pcre_builtin!(exact_preg_last_error => preg_last_error);
exact_pcre_builtin!(exact_preg_last_error_msg => preg_last_error_msg);

#[derive(Debug)]
pub struct NativePregPublishedMatch<T> {
    pub matched: bool,
    pub captures: Option<T>,
}

#[derive(Debug)]
pub struct NativePregPublishedMatchAll<T> {
    pub count: i64,
    pub captures: Option<T>,
}

/// PCRE-specific extension of the structured native sink. Capture rows can
/// publish both a named and numeric key for one owned value, while pattern
/// order appends directly into independently growing native columns.
#[doc(hidden)]
pub trait NativePregCapturePublisher: super::json::NativeStructuredValuePublisher {
    fn publish_preg_capture(
        &mut self,
        bytes: Option<&[u8]>,
        offset: Option<i64>,
        unmatched_as_null: bool,
    ) -> Option<Self::Output>
    where
        Self: Sized,
    {
        let value = match bytes {
            Some(bytes) => self.publish_string(bytes)?,
            None if unmatched_as_null => self.publish_null()?,
            None => self.publish_string(&[])?,
        };
        let Some(offset) = offset else {
            return Some(value);
        };
        let mut value = Some(value);
        self.publish_array_with(2, |publisher, index| match index {
            0 => value.take(),
            1 => publisher.publish_int(offset),
            _ => None,
        })
    }

    fn publish_preg_capture_row<'a, E>(
        &mut self,
        length: usize,
        build: impl FnMut(&mut Self, usize) -> Result<(Option<&'a [u8]>, Self::Output), E>,
    ) -> Result<Option<Self::Output>, E>
    where
        Self: Sized;

    fn publish_preg_capture_columns<E>(
        &mut self,
        groups: usize,
        build: impl FnOnce(
            &mut Self,
            &mut dyn FnMut(&mut Self, usize, Self::Output) -> Option<()>,
        ) -> Result<(), E>,
    ) -> Result<Option<Self::Output>, E>
    where
        Self: Sized;
}

#[derive(Debug)]
pub struct NativePregReplaceResult {
    pub bytes: Option<Vec<u8>>,
    pub count: i64,
}

/// Typed outcome of preparing one native callback replacement plan.
///
/// `SemanticFailure` is PHP's normal `null` result with `preg_last_error`
/// already updated. `Unsupported` is reserved for representation or
/// publication failures that may take the one pre-effect baseline
/// continuation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativePregCallbackPlanResult<T> {
    Plan(T),
    SemanticFailure,
    Unsupported,
}

/// Publishes the complete scalar `preg_replace_callback()` match plan without
/// invoking or representing the PHP callback. Every outer row is
/// `[start, end, captures]`; `captures` has the exact PHP-visible named and
/// numeric capture layout. Generated code can therefore invoke one already
/// prepared compiled callback per row and assemble the output without
/// entering builtin dispatch or constructing a Rust `Value`.
#[doc(hidden)]
pub fn native_preg_callback_plan_into<P: NativePregCapturePublisher>(
    state: &mut crate::builtins::PcreRequestState,
    limits: pcre::PcreMatchLimits,
    pattern: &[u8],
    subject: &[u8],
    limit: i64,
    flags: i64,
    publisher: &mut P,
) -> NativePregCallbackPlanResult<P::Output> {
    let compiled = match state.cache_mut().compile_bytes_with_limits(pattern, limits) {
        Ok(compiled) => compiled,
        // Pattern-compilation failures emit a PHP warning. The caller may
        // take its one baseline continuation only before callback effects.
        Err(_) => return NativePregCallbackPlanResult::Unsupported,
    };
    let options = match state
        .cache_mut()
        .match_options_for_subject_bytes_at_offset(&compiled, subject, 0)
    {
        Ok(options) => options,
        Err(error) => {
            state
                .last_error_mut()
                .set(error.code(), pcre::preg_error_message(error.code()));
            return NativePregCallbackPlanResult::SemanticFailure;
        }
    };
    let mut emitted = 0i64;
    let mut publication_failed = false;
    let mut match_failure = None;
    let published = publisher.publish_array_stream::<()>(|publisher, push| {
        let walked = compiled.for_each_php_match_with_options(
            subject,
            0,
            options,
            |captures| {
                let Some(full) = captures.get(0) else {
                    return Ok(true);
                };
                if limit >= 0 && emitted >= limit {
                    return Ok(false);
                }
                let capture_count = if flags & pcre::PREG_UNMATCHED_AS_NULL != 0 {
                    captures.len()
                } else {
                    (0..captures.len())
                        .rev()
                        .find(|index| captures.get(*index).is_some())
                        .map_or(0, |index| index + 1)
                };
                let captures =
                    match publisher.publish_preg_capture_row(capture_count, |publisher, index| {
                        let capture = captures.get(index);
                        let bytes =
                            capture
                                .as_ref()
                                .map(|capture| capture.as_bytes())
                                .or_else(|| {
                                    (flags & pcre::PREG_UNMATCHED_AS_NULL == 0).then_some(&[][..])
                                });
                        let offset = (flags & pcre::PREG_OFFSET_CAPTURE != 0).then(|| {
                            capture
                                .as_ref()
                                .map_or(-1, |capture| capture.start() as i64)
                        });
                        let value = publisher
                            .publish_preg_capture(
                                bytes,
                                offset,
                                flags & pcre::PREG_UNMATCHED_AS_NULL != 0,
                            )
                            .ok_or(())?;
                        let name = compiled
                            .capture_names()
                            .get(index)
                            .and_then(Option::as_ref)
                            .map(|name| name.as_bytes());
                        Ok((name, value))
                    }) {
                        Ok(Some(captures)) => captures,
                        Ok(None) | Err(()) => {
                            publication_failed = true;
                            return Ok(false);
                        }
                    };
                let Some(start) = publisher.publish_int(full.start() as i64) else {
                    publisher.rollback(captures);
                    publication_failed = true;
                    return Ok(false);
                };
                let Some(end) = publisher.publish_int(full.end() as i64) else {
                    publisher.rollback(start);
                    publisher.rollback(captures);
                    publication_failed = true;
                    return Ok(false);
                };
                let mut row = [Some(start), Some(end), Some(captures)];
                let Some(row) =
                    publisher.publish_array_with(row.len(), |_, index| row[index].take())
                else {
                    publication_failed = true;
                    return Ok(false);
                };
                if push(publisher, row).is_none() {
                    publication_failed = true;
                    return Ok(false);
                }
                emitted += 1;
                Ok(true)
            },
            std::convert::identity,
        );
        if let Err(error) = walked {
            match_failure = Some(error);
            Err(())
        } else if publication_failed {
            Err(())
        } else {
            Ok(())
        }
    });
    if let Some(error) = match_failure {
        state
            .last_error_mut()
            .set(error.code(), pcre::preg_error_message(error.code()));
        return NativePregCallbackPlanResult::SemanticFailure;
    }
    let plan = match published {
        Ok(Some(plan)) => plan,
        Ok(None) | Err(()) => return NativePregCallbackPlanResult::Unsupported,
    };
    state.last_error_mut().clear();
    NativePregCallbackPlanResult::Plan(plan)
}

/// Runs `preg_match` over native bytes and publishes each borrowed capture
/// directly into the authoritative native sink.
#[doc(hidden)]
// Architecture: this is the complete typed native preg_match boundary; grouping
// these publication inputs would only add a second adapter ABI.
#[allow(clippy::too_many_arguments)]
pub fn native_preg_match_into<P: NativePregCapturePublisher>(
    state: &mut crate::builtins::PcreRequestState,
    limits: pcre::PcreMatchLimits,
    pattern: &[u8],
    subject: &[u8],
    flags: i64,
    offset: i64,
    publish_captures: bool,
    publisher: &mut P,
) -> Result<Option<NativePregPublishedMatch<P::Output>>, BuiltinError> {
    validate_preg_offset_min("preg_match", offset)?;
    validate_preg_match_flags("preg_match", "#4 ($flags)", flags)?;
    let Some(start) = preg_match_offset(subject.len(), offset) else {
        return Ok(None);
    };
    let compiled = match state.cache_mut().compile_bytes_with_limits(pattern, limits) {
        Ok(compiled) => compiled,
        Err(_) => return Ok(None),
    };
    let mut capture_names = std::collections::BTreeSet::new();
    if compiled
        .capture_names()
        .iter()
        .filter_map(Option::as_deref)
        .any(|name| !capture_names.insert(name))
    {
        // Duplicate-name unmatched precedence is PHP-visible. Keep that rare
        // PCRE extension shape at the one baseline continuation.
        return Ok(None);
    }
    let options = match state
        .cache_mut()
        .match_options_for_subject_bytes_at_offset(&compiled, subject, start)
    {
        Ok(options) => options,
        Err(_) => return Ok(None),
    };
    let captures = match compiled.captures_at_with_options(subject, start, options) {
        Ok(captures) => captures,
        Err(_) => return Ok(None),
    };
    let Some(captures) = captures else {
        let captures = if publish_captures {
            match publisher.publish_preg_capture_row::<()>(0, |_, _| unreachable!()) {
                Ok(Some(captures)) => Some(captures),
                Ok(None) | Err(()) => return Ok(None),
            }
        } else {
            None
        };
        state.last_error_mut().clear();
        return Ok(Some(NativePregPublishedMatch {
            matched: false,
            captures,
        }));
    };
    if captures.mark().is_some() {
        return Ok(None);
    }
    let count = if flags & pcre::PREG_UNMATCHED_AS_NULL != 0 {
        captures.len()
    } else {
        (0..captures.len())
            .rev()
            .find(|index| captures.get(*index).is_some())
            .map_or(0, |index| index + 1)
    };
    let published = if publish_captures {
        match publisher.publish_preg_capture_row(count, |publisher, index| {
            let capture = captures.get(index);
            let bytes = capture
                .as_ref()
                .map(|capture| capture.as_bytes())
                .or_else(|| (flags & pcre::PREG_UNMATCHED_AS_NULL == 0).then_some(&[][..]));
            let offset = (flags & pcre::PREG_OFFSET_CAPTURE != 0).then(|| {
                capture
                    .as_ref()
                    .map_or(-1, |capture| capture.start() as i64)
            });
            let value = publisher
                .publish_preg_capture(bytes, offset, flags & pcre::PREG_UNMATCHED_AS_NULL != 0)
                .ok_or(())?;
            let name = compiled
                .capture_names()
                .get(index)
                .and_then(Option::as_ref)
                .map(|name| name.as_bytes());
            Ok((name, value))
        }) {
            Ok(Some(captures)) => Some(captures),
            Ok(None) | Err(()) => return Ok(None),
        }
    } else {
        None
    };
    state.last_error_mut().clear();
    Ok(Some(NativePregPublishedMatch {
        matched: true,
        captures: published,
    }))
}

#[doc(hidden)]
// Architecture: this is the complete typed native preg_match_all boundary;
// grouping these publication inputs would only add a second adapter ABI.
#[allow(clippy::too_many_arguments)]
pub fn native_preg_match_all_into<P: NativePregCapturePublisher>(
    state: &mut crate::builtins::PcreRequestState,
    limits: pcre::PcreMatchLimits,
    pattern: &[u8],
    subject: &[u8],
    flags: i64,
    offset: i64,
    publish_captures: bool,
    publisher: &mut P,
) -> Result<Option<NativePregPublishedMatchAll<P::Output>>, BuiltinError> {
    validate_preg_match_all_flags(flags)?;
    validate_preg_offset_min("preg_match_all", offset)?;
    let Some(start) = preg_match_offset(subject.len(), offset) else {
        return Ok(None);
    };
    let compiled = match state.cache_mut().compile_bytes_with_limits(pattern, limits) {
        Ok(compiled) => compiled,
        Err(_) => return Ok(None),
    };
    if compiled.capture_names().iter().any(Option::is_some) {
        return Ok(None);
    }
    let options = match state
        .cache_mut()
        .match_options_for_subject_bytes_at_offset(&compiled, subject, start)
    {
        Ok(options) => options,
        Err(_) => return Ok(None),
    };
    let set_order = flags & pcre::PREG_SET_ORDER != 0;
    let mut unsupported = false;
    let mut count = 0_i64;
    let captures = if !publish_captures {
        let walked = compiled.for_each_php_match_with_options(
            subject,
            start,
            options,
            |captures| {
                if captures.mark().is_some() {
                    unsupported = true;
                    return Ok(false);
                }
                count = count.saturating_add(1);
                Ok(true)
            },
            std::convert::identity,
        );
        if walked.is_err() || unsupported {
            return Ok(None);
        }
        None
    } else if set_order {
        let mut publication_failed = false;
        let published = publisher.publish_array_stream::<()>(|publisher, push| {
            let walked = compiled.for_each_php_match_with_options(
                subject,
                start,
                options,
                |captures| {
                    if captures.mark().is_some() {
                        unsupported = true;
                        return Ok(false);
                    }
                    let row_length = if flags & pcre::PREG_UNMATCHED_AS_NULL != 0 {
                        captures.len()
                    } else {
                        (0..captures.len())
                            .rev()
                            .find(|index| captures.get(*index).is_some())
                            .map_or(0, |index| index + 1)
                    };
                    let row = publisher.publish_preg_capture_row(row_length, |publisher, index| {
                        let capture = captures.get(index);
                        let bytes =
                            capture
                                .as_ref()
                                .map(|capture| capture.as_bytes())
                                .or_else(|| {
                                    (flags & pcre::PREG_UNMATCHED_AS_NULL == 0).then_some(&[][..])
                                });
                        let offset = (flags & pcre::PREG_OFFSET_CAPTURE != 0).then(|| {
                            capture
                                .as_ref()
                                .map_or(-1, |capture| capture.start() as i64)
                        });
                        publisher
                            .publish_preg_capture(
                                bytes,
                                offset,
                                flags & pcre::PREG_UNMATCHED_AS_NULL != 0,
                            )
                            .map(|value| (None, value))
                            .ok_or(())
                    });
                    let row = match row {
                        Ok(Some(row)) => row,
                        Ok(None) | Err(()) => {
                            publication_failed = true;
                            return Ok(false);
                        }
                    };
                    if push(publisher, row).is_none() {
                        publication_failed = true;
                        return Ok(false);
                    }
                    count = count.saturating_add(1);
                    Ok(true)
                },
                std::convert::identity,
            );
            if walked.is_err() || unsupported || publication_failed {
                Err(())
            } else {
                Ok(())
            }
        });
        match published {
            Ok(Some(captures)) => Some(captures),
            Ok(None) | Err(()) => return Ok(None),
        }
    } else {
        let groups = compiled.capture_names().len();
        let mut publication_failed = false;
        let published = publisher.publish_preg_capture_columns::<()>(groups, |publisher, push| {
            let walked = compiled.for_each_php_match_with_options(
                subject,
                start,
                options,
                |captures| {
                    if captures.mark().is_some() {
                        unsupported = true;
                        return Ok(false);
                    }
                    for index in 0..groups {
                        let capture = captures.get(index);
                        let bytes =
                            capture
                                .as_ref()
                                .map(|capture| capture.as_bytes())
                                .or_else(|| {
                                    (flags & pcre::PREG_UNMATCHED_AS_NULL == 0).then_some(&[][..])
                                });
                        let offset = (flags & pcre::PREG_OFFSET_CAPTURE != 0).then(|| {
                            capture
                                .as_ref()
                                .map_or(-1, |capture| capture.start() as i64)
                        });
                        let Some(value) = publisher.publish_preg_capture(
                            bytes,
                            offset,
                            flags & pcre::PREG_UNMATCHED_AS_NULL != 0,
                        ) else {
                            publication_failed = true;
                            return Ok(false);
                        };
                        if push(publisher, index, value).is_none() {
                            publication_failed = true;
                            return Ok(false);
                        }
                    }
                    count = count.saturating_add(1);
                    Ok(true)
                },
                std::convert::identity,
            );
            if walked.is_err() || unsupported || publication_failed {
                Err(())
            } else {
                Ok(())
            }
        });
        match published {
            Ok(Some(captures)) => Some(captures),
            Ok(None) | Err(()) => return Ok(None),
        }
    };
    state.last_error_mut().clear();
    Ok(Some(NativePregPublishedMatchAll { count, captures }))
}

/// Executes the scalar form shared by `preg_replace` and `preg_filter`
/// directly over native bytes, including capture expansion.
#[doc(hidden)]
pub fn native_preg_replace_scalar(
    state: &mut crate::builtins::PcreRequestState,
    limits: pcre::PcreMatchLimits,
    pattern: &[u8],
    replacement: &[u8],
    subject: &[u8],
    limit: i64,
    filter: bool,
) -> Option<NativePregReplaceResult> {
    let compiled = state
        .cache_mut()
        .compile_bytes_with_limits(pattern, limits)
        .ok()?;
    let mut count = 0;
    let bytes = preg_replace_bytes(&compiled, replacement, subject, limit, &mut count).ok()?;
    state.last_error_mut().clear();
    Some(NativePregReplaceResult {
        bytes: (!filter || count != 0).then_some(bytes),
        count,
    })
}

/// Executes one prepared scalar pattern/replacement over a direct array's
/// string subjects. Keys remain authoritative in the caller; this returns
/// only replacement bytes and the aggregate replacement count.
#[doc(hidden)]
// Architecture: this is the complete typed native multi-subject replacement
// boundary; grouping these publication inputs would only add a second adapter.
#[allow(clippy::too_many_arguments)]
pub fn native_preg_replace_many_into<'a, E>(
    state: &mut crate::builtins::PcreRequestState,
    limits: pcre::PcreMatchLimits,
    pattern: &[u8],
    replacement: &[u8],
    subject_count: usize,
    mut subject_at: impl FnMut(usize) -> Option<&'a [u8]>,
    limit: i64,
    filter: bool,
    mut publish: impl FnMut(usize, Option<&[u8]>) -> Result<(), E>,
) -> Result<Option<i64>, E> {
    let compiled = match state.cache_mut().compile_bytes_with_limits(pattern, limits) {
        Ok(compiled) => compiled,
        Err(_) => return Ok(None),
    };
    let mut count = 0;
    for index in 0..subject_count {
        let Some(subject) = subject_at(index) else {
            return Ok(None);
        };
        let before = count;
        let replaced = match preg_replace_bytes(&compiled, replacement, subject, limit, &mut count)
        {
            Ok(replaced) => replaced,
            Err(_) => return Ok(None),
        };
        publish(index, (!filter || count != before).then_some(&replaced))?;
    }
    state.last_error_mut().clear();
    Ok(Some(count))
}

#[doc(hidden)]
pub fn native_preg_split_into<P: NativePregCapturePublisher>(
    state: &mut crate::builtins::PcreRequestState,
    limits: pcre::PcreMatchLimits,
    pattern: &[u8],
    subject: &[u8],
    limit: i64,
    flags: i64,
    publisher: &mut P,
) -> Option<P::Output> {
    let compiled = state
        .cache_mut()
        .compile_bytes_with_limits(pattern, limits)
        .ok()?;
    let options = state
        .cache_mut()
        .match_options_for_subject_bytes_at_offset(&compiled, subject, 0)
        .ok()?;
    let mut last_end = 0usize;
    let mut emitted = 0i64;
    let mut publication_failed = false;
    let published = publisher.publish_array_stream::<()>(|publisher, push| {
        let mut append = |publisher: &mut P, bytes: &[u8], offset: usize| {
            if flags & pcre::PREG_SPLIT_NO_EMPTY != 0 && bytes.is_empty() {
                return true;
            }
            let value = publisher.publish_preg_capture(
                Some(bytes),
                (flags & pcre::PREG_SPLIT_OFFSET_CAPTURE != 0).then_some(offset as i64),
                false,
            );
            let Some(value) = value else {
                return false;
            };
            push(publisher, value).is_some()
        };
        let walked = compiled.for_each_php_match_with_options(
            subject,
            0,
            options,
            |captures| {
                let Some(full) = captures.get(0) else {
                    return Ok(true);
                };
                if limit > 0 && emitted >= limit - 1 {
                    return Ok(false);
                }
                if full.start() < last_end {
                    return Err(pcre::PcreFailure::new(
                        pcre::PREG_INTERNAL_ERROR,
                        "PCRE split match moved before the previous delimiter",
                    ));
                }
                if !append(publisher, &subject[last_end..full.start()], last_end) {
                    publication_failed = true;
                    return Ok(false);
                }
                emitted += 1;
                if flags & pcre::PREG_SPLIT_DELIM_CAPTURE != 0 {
                    for index in 1..captures.len() {
                        if let Some(capture) = captures.get(index)
                            && !append(publisher, capture.as_bytes(), capture.start())
                        {
                            publication_failed = true;
                            return Ok(false);
                        }
                    }
                }
                last_end = full.end();
                Ok(true)
            },
            std::convert::identity,
        );
        if walked.is_err()
            || publication_failed
            || !append(publisher, &subject[last_end..], last_end)
        {
            Err(())
        } else {
            Ok(())
        }
    });
    let captures = match published {
        Ok(Some(captures)) => captures,
        Ok(None) | Err(()) => return None,
    };
    state.last_error_mut().clear();
    Some(captures)
}

/// Selects the input strings matched by `preg_grep` without constructing a
/// PHP array or PHP string representation. The caller keeps the authoritative
/// keys and values and uses this mask to publish the result array directly.
#[doc(hidden)]
pub fn native_preg_grep_into<'a, E>(
    state: &mut crate::builtins::PcreRequestState,
    limits: pcre::PcreMatchLimits,
    pattern: &[u8],
    subject_count: usize,
    mut subject_at: impl FnMut(usize) -> Option<&'a [u8]>,
    flags: i64,
    mut publish: impl FnMut(usize) -> Result<(), E>,
) -> Result<Option<()>, E> {
    let compiled = match state.cache_mut().compile_bytes_with_limits(pattern, limits) {
        Ok(compiled) => compiled,
        Err(_) => return Ok(None),
    };
    let invert = flags & pcre::PREG_GREP_INVERT != 0;
    for index in 0..subject_count {
        let Some(subject) = subject_at(index) else {
            return Ok(None);
        };
        let options = match state
            .cache_mut()
            .match_options_for_subject_bytes_at_offset(&compiled, subject, 0)
        {
            Ok(options) => options,
            Err(_) => return Ok(None),
        };
        let is_match = match compiled.captures_at_with_options(subject, 0, options) {
            Ok(captures) => captures.is_some(),
            Err(_) => return Ok(None),
        };
        if is_match != invert {
            publish(index)?;
        }
    }
    state.last_error_mut().clear();
    Ok(Some(()))
}

pub(in crate::builtins::modules) fn builtin_preg_replace_callback(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    let mut services = context.pcre_callback_services();
    preg_replace_callback(&mut services, args, span)
}

fn preg_match(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 5 {
        return Err(arity_error("preg_match", "two to five argument(s)"));
    }
    let pattern = string_needle_arg("preg_match", "#1 ($pattern)", &args[0])?;
    let subject = string_arg("preg_match", &args[1])?;
    let flags = args
        .get(3)
        .map(|value| int_arg("preg_match", value))
        .transpose()?
        .unwrap_or(0);
    let offset = args
        .get(4)
        .map(|value| int_arg("preg_match", value))
        .transpose()?
        .unwrap_or(0);
    validate_preg_offset_min("preg_match", offset)?;
    let subject_bytes = subject.as_bytes();
    let Some(start_offset) = preg_match_offset(subject_bytes.len(), offset) else {
        context.set_preg_last_error(
            pcre::PREG_INTERNAL_ERROR,
            pcre::preg_error_message(pcre::PREG_INTERNAL_ERROR),
        );
        assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
        return Ok(Value::Bool(false));
    };
    if let Ok(Some(literal)) = pcre::simple_literal_pattern(&pattern) {
        validate_preg_match_flags("preg_match", "#4 ($flags)", flags)?;
        return match find_literal_match(subject_bytes, literal.as_bytes(), start_offset) {
            Some((start, end)) => {
                assign_reference_arg(
                    args.get(2),
                    preg_single_capture_array(subject_bytes, start, end, flags),
                );
                context.clear_preg_last_error();
                Ok(Value::Int(1))
            }
            None => {
                assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
                context.clear_preg_last_error();
                Ok(Value::Int(0))
            }
        };
    }
    let Some(compiled) = compile_preg_pattern(context, "preg_match", pattern, span) else {
        return Ok(Value::Bool(false));
    };
    validate_preg_match_flags("preg_match", "#4 ($flags)", flags)?;
    let match_options = match context.pcre_cache().match_options_for_subject_at_offset(
        &compiled,
        &subject,
        start_offset,
    ) {
        Ok(options) => options,
        Err(error) => {
            assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
            return preg_failure(context, error);
        }
    };
    if let Some(fast_match) = compiled.fast_match_at(subject_bytes, start_offset) {
        return match fast_match {
            Some((start, end)) => {
                assign_reference_arg(
                    args.get(2),
                    preg_single_capture_array(subject_bytes, start, end, flags),
                );
                context.clear_preg_last_error();
                Ok(Value::Int(1))
            }
            None => {
                assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
                context.clear_preg_last_error();
                Ok(Value::Int(0))
            }
        };
    }
    match compiled.captures_at_with_options(subject_bytes, start_offset, match_options) {
        Ok(Some(captures)) => {
            let matches =
                pcre::captures_to_array_with_names(&captures, compiled.capture_names(), flags, 0);
            assign_reference_arg(args.get(2), matches);
            context.clear_preg_last_error();
            Ok(Value::Int(1))
        }
        Ok(None) => {
            assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
            context.clear_preg_last_error();
            Ok(Value::Int(0))
        }
        Err(error) => preg_failure(context, error),
    }
}

fn preg_single_capture_array(subject: &[u8], start: usize, end: usize, flags: i64) -> Value {
    let matched = Value::String(PhpString::intern(&subject[start..end]));
    let capture = if flags & pcre::PREG_OFFSET_CAPTURE != 0 {
        Value::packed_array(vec![matched, Value::Int(start as i64)])
    } else {
        matched
    };
    Value::packed_array(vec![capture])
}

fn find_literal_match(haystack: &[u8], needle: &[u8], start: usize) -> Option<(usize, usize)> {
    if needle.is_empty() || start > haystack.len() {
        return None;
    }
    let last_start = haystack.len().checked_sub(needle.len())?;
    let first = needle[0];
    let mut index = start;
    while index <= last_start {
        if haystack[index] == first && &haystack[index..index + needle.len()] == needle {
            return Some((index, index + needle.len()));
        }
        index += 1;
    }
    None
}

fn preg_match_all(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 5 {
        return Err(arity_error("preg_match_all", "two to five argument(s)"));
    }
    let pattern = string_needle_arg("preg_match_all", "#1 ($pattern)", &args[0])?;
    let subject = string_arg("preg_match_all", &args[1])?;
    let flags = args
        .get(3)
        .map(|value| int_arg("preg_match_all", value))
        .transpose()?
        .unwrap_or(pcre::PREG_PATTERN_ORDER);
    validate_preg_match_all_flags(flags)?;
    let offset = args
        .get(4)
        .map(|value| int_arg("preg_match_all", value))
        .transpose()?
        .unwrap_or(0);
    validate_preg_offset_min("preg_match_all", offset)?;
    let subject_bytes = subject.as_bytes();
    let Some(start_offset) = preg_match_offset(subject_bytes.len(), offset) else {
        context.set_preg_last_error(
            pcre::PREG_INTERNAL_ERROR,
            pcre::preg_error_message(pcre::PREG_INTERNAL_ERROR),
        );
        assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
        return Ok(Value::Bool(false));
    };
    let Some(compiled) = compile_preg_pattern(context, "preg_match_all", pattern, span) else {
        return Ok(Value::Bool(false));
    };
    let match_options = match context.pcre_cache().match_options_for_subject_at_offset(
        &compiled,
        &subject,
        start_offset,
    ) {
        Ok(options) => options,
        Err(error) => {
            assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
            return preg_failure(context, error);
        }
    };

    let set_order = flags & pcre::PREG_SET_ORDER != 0;
    let mut all = Vec::new();
    if let Err(error) = compiled.for_each_php_match_with_options(
        subject_bytes,
        start_offset,
        match_options,
        |captures| {
            all.push(pcre::captures_to_array_with_names_for_order(
                &captures,
                compiled.capture_names(),
                flags,
                0,
                set_order,
            ));
            Ok(true)
        },
        std::convert::identity,
    ) {
        return preg_failure(context, error);
    }
    let count = all.len() as i64;
    let output = if set_order {
        Value::packed_array(all)
    } else {
        pattern_order_matches(all, compiled.capture_names())
    };
    assign_reference_arg(args.get(2), output);
    context.clear_preg_last_error();
    Ok(Value::Int(count))
}

fn preg_match_offset(subject_len: usize, offset: i64) -> Option<usize> {
    if offset >= 0 {
        let offset = offset as usize;
        return (offset <= subject_len).then_some(offset);
    }
    Some(subject_len.saturating_sub(offset.unsigned_abs() as usize))
}

fn validate_preg_offset_min(function: &str, offset: i64) -> Result<(), BuiltinError> {
    if offset == i64::MIN {
        return Err(argument_value_error(
            function,
            "#5 ($offset)",
            &format!("must be greater than {}", i64::MIN),
        ));
    }
    Ok(())
}

fn validate_preg_match_flags(
    function: &str,
    argument: &str,
    flags: i64,
) -> Result<(), BuiltinError> {
    const VALID_FLAGS: i64 = pcre::PREG_OFFSET_CAPTURE | pcre::PREG_UNMATCHED_AS_NULL;
    if flags & !VALID_FLAGS != 0 {
        return Err(argument_value_error(
            function,
            argument,
            "must be a PREG_* constant",
        ));
    }
    Ok(())
}

fn validate_preg_match_all_flags(flags: i64) -> Result<(), BuiltinError> {
    const VALID_FLAGS: i64 = pcre::PREG_PATTERN_ORDER
        | pcre::PREG_SET_ORDER
        | pcre::PREG_OFFSET_CAPTURE
        | pcre::PREG_UNMATCHED_AS_NULL;
    let order_flags = flags & (pcre::PREG_PATTERN_ORDER | pcre::PREG_SET_ORDER);
    if flags & !VALID_FLAGS != 0 || order_flags == (pcre::PREG_PATTERN_ORDER | pcre::PREG_SET_ORDER)
    {
        return Err(argument_value_error(
            "preg_match_all",
            "#4 ($flags)",
            "must be a PREG_* constant",
        ));
    }
    Ok(())
}
fn preg_replace(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 3 || args.len() > 5 {
        return Err(arity_error("preg_replace", "three to five argument(s)"));
    }
    let limit = args
        .get(3)
        .map(|value| int_arg("preg_replace", value))
        .transpose()?
        .unwrap_or(-1);
    if let Some(result) = preg_replace_simple_literal_scalar(context, &args, limit)? {
        return Ok(result);
    }
    let Some(specs) = preg_replace_specs(context, "preg_replace", &args[0], &args[1], span)? else {
        return Ok(Value::Null);
    };
    let mut count = 0;
    let result = match preg_replace_subject_with_specs(&specs, &args[2], limit, &mut count) {
        Ok(result) => result,
        Err(error) => return preg_replace_failure(context, error),
    };
    assign_reference_arg(args.get(4), Value::Int(count));
    context.clear_preg_last_error();
    Ok(result)
}

fn preg_replace_simple_literal_scalar(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: &[Value],
    limit: i64,
) -> Result<Option<Value>, BuiltinError> {
    let pattern = match deref_value(&args[0]) {
        Value::String(pattern) => pattern,
        _ => return Ok(None),
    };
    let replacement = match deref_value(&args[1]) {
        Value::String(replacement) => replacement,
        _ => return Ok(None),
    };
    let subject = match deref_value(&args[2]) {
        Value::String(subject) => subject,
        _ => return Ok(None),
    };
    if replacement
        .as_bytes()
        .iter()
        .any(|byte| matches!(*byte, b'$' | b'\\'))
    {
        return Ok(None);
    }
    let Ok(Some(literal)) = pcre::simple_literal_pattern(&pattern) else {
        return Ok(None);
    };
    let mut count = 0i64;
    let replaced = replace_literal_bytes(
        subject.as_bytes(),
        literal.as_bytes(),
        replacement.as_bytes(),
        limit,
        &mut count,
    );
    assign_reference_arg(args.get(4), Value::Int(count));
    context.clear_preg_last_error();
    Ok(Some(Value::string(replaced)))
}

fn replace_literal_bytes(
    subject: &[u8],
    needle: &[u8],
    replacement: &[u8],
    limit: i64,
    count: &mut i64,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(subject.len());
    let mut cursor = 0usize;
    while cursor <= subject.len() {
        if limit >= 0 && *count >= limit {
            break;
        }
        let Some((start, end)) = find_literal_match(subject, needle, cursor) else {
            break;
        };
        output.extend_from_slice(&subject[cursor..start]);
        output.extend_from_slice(replacement);
        cursor = end;
        *count += 1;
    }
    output.extend_from_slice(&subject[cursor..]);
    output
}

fn preg_filter(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 3 || args.len() > 5 {
        return Err(arity_error("preg_filter", "three to five argument(s)"));
    }
    let limit = args
        .get(3)
        .map(|value| int_arg("preg_filter", value))
        .transpose()?
        .unwrap_or(-1);
    let Some(specs) = preg_replace_specs(context, "preg_filter", &args[0], &args[1], span)? else {
        return Ok(Value::Null);
    };
    let mut count = 0;
    let result = match preg_replace_filter_subject_with_specs(&specs, &args[2], limit, &mut count) {
        Ok(result) => result,
        Err(error) => return preg_replace_failure(context, error),
    };
    assign_reference_arg(args.get(4), Value::Int(count));
    context.clear_preg_last_error();
    Ok(result)
}

fn preg_replace_specs(
    context: &mut PcreBuiltinServices<'_, '_>,
    function_name: &str,
    pattern: &Value,
    replacement: &Value,
    span: RuntimeSourceSpan,
) -> Result<Option<Vec<PregReplaceSpec>>, BuiltinError> {
    let replacement_array = match deref_value(replacement) {
        Value::Array(array) => Some(array),
        _ => None,
    };

    let patterns = match deref_value(pattern) {
        Value::Array(array) => {
            let mut patterns = Vec::new();
            for (_, value) in array.iter() {
                patterns.push(string_arg(function_name, value)?);
            }
            patterns
        }
        _ if replacement_array.is_some() => {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_BUILTIN_TYPE",
                format!(
                    "{function_name}(): Argument #1 ($pattern) must be of type array when argument #2 ($replacement) is an array, {} given",
                    php_argument_type_name(pattern)
                ),
            ));
        }
        _ => vec![string_arg(function_name, pattern).map_err(|_| {
            argument_type_error(function_name, "#1 ($pattern)", "array|string", pattern)
        })?],
    };

    let replacements = if let Some(array) = replacement_array {
        let mut replacements = Vec::new();
        for (_, value) in array.iter() {
            replacements.push(string_arg(function_name, value)?.into_bytes());
        }
        PregReplaceReplacements::Array(replacements)
    } else {
        PregReplaceReplacements::Scalar(
            string_arg(function_name, replacement)
                .map_err(|_| {
                    argument_type_error(
                        function_name,
                        "#2 ($replacement)",
                        "array|string",
                        replacement,
                    )
                })?
                .into_bytes(),
        )
    };

    let mut specs = Vec::new();
    for (index, pattern) in patterns.into_iter().enumerate() {
        let Some(compiled) = compile_preg_pattern(context, function_name, pattern, span.clone())
        else {
            return Ok(None);
        };
        let replacement = replacements.get(index).to_vec();
        specs.push((compiled, replacement));
    }
    Ok(Some(specs))
}

enum PregReplaceReplacements {
    Scalar(Vec<u8>),
    Array(Vec<Vec<u8>>),
}

impl PregReplaceReplacements {
    fn get(&self, index: usize) -> &[u8] {
        match self {
            Self::Scalar(value) => value,
            Self::Array(values) => values.get(index).map_or(b"".as_slice(), Vec::as_slice),
        }
    }
}
fn preg_replace_callback(
    context: &mut PcreCallbackServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 3 || args.len() > 6 {
        return Err(arity_error(
            "preg_replace_callback",
            "three to six argument(s)",
        ));
    }
    let pattern = string_arg("preg_replace_callback", &args[0])?;
    let limit = args
        .get(3)
        .map(|value| int_arg("preg_replace_callback", value))
        .transpose()?
        .unwrap_or(-1);
    let flags = args
        .get(5)
        .map(|value| int_arg("preg_replace_callback", value))
        .transpose()?
        .unwrap_or(0);
    let callback_name = match deref_value(&args[1]).as_callable() {
        Some(CallableValue::InternalBuiltin { name }) => name.clone(),
        _ => {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_CALLABLE_CONTEXT_REQUIRED",
                "preg_replace_callback requires VM callable dispatch for user callbacks",
            ));
        }
    };
    let Some(callback) = BuiltinRegistry::new().get(&callback_name) else {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_UNDEFINED_CALLBACK",
            format!("Undefined callback `{callback_name}`"),
        ));
    };
    let Some(compiled) =
        compile_preg_pattern(context, "preg_replace_callback", pattern, span.clone())
    else {
        return Ok(Value::Null);
    };
    let mut count = 0;
    let result = preg_replace_callback_subject(
        context, &compiled, callback, &args[2], limit, flags, &mut count, span,
    )?;
    if matches!(result, Value::Null) && context.preg_last_error().0 != pcre::PREG_NO_ERROR {
        assign_reference_arg(args.get(4), Value::Int(count));
        return Ok(Value::Null);
    }
    assign_reference_arg(args.get(4), Value::Int(count));
    context.clear_preg_last_error();
    Ok(result)
}

pub(in crate::builtins::modules) fn builtin_preg_replace_callback_array(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 6 {
        return Err(arity_error(
            "preg_replace_callback_array",
            "two to six argument(s)",
        ));
    }
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_CALLABLE_CONTEXT_REQUIRED",
        "preg_replace_callback_array requires VM callable dispatch for user callbacks",
    ))
}

fn preg_split(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 4 {
        return Err(arity_error("preg_split", "two to four argument(s)"));
    }
    let pattern = string_arg("preg_split", &args[0])?;
    let subject = string_arg("preg_split", &args[1])?;
    let limit = args
        .get(2)
        .map(|value| int_arg("preg_split", value))
        .transpose()?
        .unwrap_or(-1);
    let flags = args
        .get(3)
        .map(|value| int_arg("preg_split", value))
        .transpose()?
        .unwrap_or(0);
    if flags & pcre::PREG_SPLIT_DELIM_CAPTURE == 0
        && let Ok(Some(literal)) = pcre::simple_literal_pattern(&pattern)
    {
        return preg_split_literal(
            context,
            subject.as_bytes(),
            literal.as_bytes(),
            limit,
            flags,
        );
    }
    let Some(compiled) = compile_preg_pattern(context, "preg_split", pattern, span) else {
        return Ok(Value::Bool(false));
    };
    if let Err(error) = context
        .pcre_cache()
        .validate_utf8_subject_for_pattern(&compiled, &subject)
    {
        return preg_failure(context, error);
    }
    let subject_bytes = subject.as_bytes();
    let can_match_empty = match compiled.is_match(b"") {
        Ok(can_match_empty) => can_match_empty,
        Err(error) => return preg_failure(context, error),
    };
    if !can_match_empty {
        match compiled.captures_at(subject_bytes, 0) {
            Ok(Some(captures)) => {
                if captures
                    .get(0)
                    .is_some_and(|full| full.start() != full.end())
                {
                    return preg_split_non_empty_matches(
                        context,
                        &compiled,
                        subject_bytes,
                        limit,
                        flags,
                    );
                }
            }
            Ok(None) => {
                let mut pieces = PhpArray::new();
                append_split_piece(&mut pieces, subject_bytes, 0, flags);
                context.clear_preg_last_error();
                return Ok(Value::Array(pieces));
            }
            Err(error) => return preg_failure(context, error),
        }
    }
    preg_split_with_empty_matches(context, &compiled, subject_bytes, limit, flags)
}

fn preg_split_non_empty_matches(
    context: &mut PcreBuiltinServices<'_, '_>,
    compiled: &pcre::CompiledPattern,
    subject_bytes: &[u8],
    limit: i64,
    flags: i64,
) -> BuiltinResult {
    let mut pieces = PhpArray::new();
    let mut last_end = 0usize;
    let mut emitted = 0i64;
    for captures in compiled.captures_iter(subject_bytes) {
        let captures = match captures {
            Ok(captures) => captures,
            Err(error) => return preg_failure(context, error.into()),
        };
        let Some(full) = captures.get(0) else {
            continue;
        };
        if limit > 0 && emitted >= limit - 1 {
            break;
        }
        append_split_piece(
            &mut pieces,
            &subject_bytes[last_end..full.start()],
            last_end,
            flags,
        );
        emitted += 1;
        if flags & pcre::PREG_SPLIT_DELIM_CAPTURE != 0 {
            for index in 1..captures.len() {
                if let Some(capture) = captures.get(index) {
                    append_split_piece(&mut pieces, capture.as_bytes(), capture.start(), flags);
                }
            }
        }
        last_end = full.end();
    }
    append_split_piece(&mut pieces, &subject_bytes[last_end..], last_end, flags);
    context.clear_preg_last_error();
    Ok(Value::Array(pieces))
}

fn preg_split_literal(
    context: &mut PcreBuiltinServices<'_, '_>,
    subject_bytes: &[u8],
    needle: &[u8],
    limit: i64,
    flags: i64,
) -> BuiltinResult {
    let mut pieces = PhpArray::new();
    let mut last_end = 0usize;
    let mut emitted = 0i64;
    while last_end <= subject_bytes.len() {
        if limit > 0 && emitted >= limit - 1 {
            break;
        }
        let Some((start, end)) = find_literal_match(subject_bytes, needle, last_end) else {
            break;
        };
        append_split_piece(
            &mut pieces,
            &subject_bytes[last_end..start],
            last_end,
            flags,
        );
        last_end = end;
        emitted += 1;
    }
    append_split_piece(&mut pieces, &subject_bytes[last_end..], last_end, flags);
    context.clear_preg_last_error();
    Ok(Value::Array(pieces))
}

fn preg_split_with_empty_matches(
    context: &mut PcreBuiltinServices<'_, '_>,
    compiled: &pcre::CompiledPattern,
    subject_bytes: &[u8],
    limit: i64,
    flags: i64,
) -> BuiltinResult {
    let mut pieces = PhpArray::new();
    let mut last_end = 0usize;
    let mut search_start = 0usize;
    let mut retry_after_empty_match = false;
    let mut retry_allows_start_reset = false;
    let mut emitted = 0i64;

    while search_start <= subject_bytes.len() {
        let captures = if retry_after_empty_match {
            let mut options = MatchOptions::default().not_empty_at_start(true);
            if !retry_allows_start_reset {
                options = options.anchored(true);
            }
            match compiled.captures_at_with_options(subject_bytes, search_start, options) {
                Ok(Some(captures)) => Some(captures),
                Ok(None) => {
                    retry_after_empty_match = false;
                    search_start = next_split_search_offset(
                        subject_bytes,
                        search_start,
                        compiled.is_utf8_mode(),
                    );
                    continue;
                }
                Err(error) => return preg_failure(context, error),
            }
        } else {
            match compiled.captures_at(subject_bytes, search_start) {
                Ok(captures) => captures,
                Err(error) => return preg_failure(context, error),
            }
        };
        let Some(captures) = captures else {
            break;
        };
        let Some(full) = captures.get(0) else {
            continue;
        };
        if limit > 0 && emitted >= limit - 1 {
            break;
        }
        if full.start() < last_end {
            return preg_failure(
                context,
                pcre::PcreFailure::new(
                    pcre::PREG_INTERNAL_ERROR,
                    "PCRE split match moved before the previous delimiter",
                ),
            );
        }
        let match_start = full.start();
        append_split_piece(
            &mut pieces,
            &subject_bytes[last_end..full.start()],
            last_end,
            flags,
        );
        emitted += 1;
        if flags & pcre::PREG_SPLIT_DELIM_CAPTURE != 0 {
            for index in 1..captures.len() {
                if let Some(capture) = captures.get(index) {
                    append_split_piece(&mut pieces, capture.as_bytes(), capture.start(), flags);
                }
            }
        }
        retry_after_empty_match = full.start() == full.end();
        retry_allows_start_reset = retry_after_empty_match && match_start > search_start;
        search_start = full.end();
        last_end = full.end();
    }
    append_split_piece(&mut pieces, &subject_bytes[last_end..], last_end, flags);
    context.clear_preg_last_error();
    Ok(Value::Array(pieces))
}

fn next_split_search_offset(subject: &[u8], offset: usize, utf8_mode: bool) -> usize {
    if offset >= subject.len() {
        return subject.len() + 1;
    }
    if !utf8_mode {
        return offset + 1;
    }
    std::str::from_utf8(&subject[offset..])
        .ok()
        .and_then(|rest| rest.chars().next())
        .map_or(offset + 1, |character| offset + character.len_utf8())
}

fn preg_grep(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("preg_grep", "two to three argument(s)"));
    }
    let pattern = string_arg("preg_grep", &args[0])?;
    let flags = args
        .get(2)
        .map(|value| int_arg("preg_grep", value))
        .transpose()?
        .unwrap_or(0);
    if let Ok(Some(literal)) = pcre::simple_literal_pattern(&pattern) {
        let Value::Array(input) = deref_value(&args[1]) else {
            return Err(type_error("preg_grep", "array", &args[1]));
        };
        let mut output = PhpArray::new();
        for (key, value) in input.iter() {
            let text = context
                .string_cast_value(value, span.clone())
                .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_TYPE_ERROR", message))?;
            let is_match = find_literal_match(text.as_bytes(), literal.as_bytes(), 0).is_some();
            if is_match != (flags & pcre::PREG_GREP_INVERT != 0) {
                output.insert(key.clone(), value.clone());
            }
        }
        context.clear_preg_last_error();
        return Ok(Value::Array(output));
    }
    let Some(compiled) = compile_preg_pattern(context, "preg_grep", pattern, span.clone()) else {
        return Ok(Value::Bool(false));
    };
    let Value::Array(input) = deref_value(&args[1]) else {
        return Err(type_error("preg_grep", "array", &args[1]));
    };
    let mut output = PhpArray::new();
    for (key, value) in input.iter() {
        let text = context
            .string_cast_value(value, span.clone())
            .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_TYPE_ERROR", message))?;
        if let Err(error) = context
            .pcre_cache()
            .validate_utf8_subject_for_pattern(&compiled, &text)
        {
            return preg_failure(context, error);
        }
        let is_match = match compiled.is_match(text.as_bytes()) {
            Ok(is_match) => is_match,
            Err(error) => return preg_failure(context, error),
        };
        if is_match != (flags & pcre::PREG_GREP_INVERT != 0) {
            output.insert(key.clone(), value.clone());
        }
    }
    context.clear_preg_last_error();
    Ok(Value::Array(output))
}
#[doc(hidden)]
pub fn exact_preg_quote(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("preg_quote", "one or two argument(s)"));
    }
    let text = string_arg("preg_quote", &args[0])?;
    let delimiter = args
        .get(1)
        .map(|value| string_arg("preg_quote", value))
        .transpose()?
        .and_then(|delimiter| delimiter.as_bytes().first().copied());
    Ok(Value::string(pcre::preg_quote(text.as_bytes(), delimiter)))
}
fn preg_last_error(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("preg_last_error", &args, 0)?;
    Ok(Value::Int(context.preg_last_error().0))
}
fn preg_last_error_msg(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("preg_last_error_msg", &args, 0)?;
    Ok(Value::string(context.preg_last_error().1))
}

fn preg_replace_failure(
    context: &mut PcreBuiltinServices<'_, '_>,
    error: pcre::PcreFailure,
) -> BuiltinResult {
    context.set_preg_last_error(error.code(), pcre::preg_error_message(error.code()));
    Ok(Value::Null)
}
