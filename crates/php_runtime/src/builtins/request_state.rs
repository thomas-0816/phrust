//! Request-owned state for builtins migrated to typed extension slots.

use super::context::CurlState;
use crate::pcre::{PcreCache, PcreLastErrorState};
use crate::{ExtensionStateLayout, ExtensionStateLayoutBuilder, ExtensionStateSlot, RequestState};
use std::sync::OnceLock;

/// Request-local PCRE cache and last-error state share one extension slot.
#[derive(Debug, Default)]
pub struct PcreRequestState {
    cache: PcreCache,
    last_error: PcreLastErrorState,
}

impl PcreRequestState {
    #[must_use]
    pub const fn cache(&self) -> &PcreCache {
        &self.cache
    }

    pub const fn cache_mut(&mut self) -> &mut PcreCache {
        &mut self.cache
    }

    #[must_use]
    pub const fn last_error(&self) -> &PcreLastErrorState {
        &self.last_error
    }

    pub const fn last_error_mut(&mut self) -> &mut PcreLastErrorState {
        &mut self.last_error
    }
}

/// Request-local JSON error state.
#[derive(Debug)]
pub struct JsonRequestState {
    code: i64,
    message: String,
}

impl Default for JsonRequestState {
    fn default() -> Self {
        Self {
            code: super::context::JSON_ERROR_NONE,
            message: super::context::json_error_message(super::context::JSON_ERROR_NONE).to_owned(),
        }
    }
}

impl JsonRequestState {
    pub fn set(&mut self, code: i64) {
        self.code = code;
        self.message = super::context::json_error_message(code).to_owned();
    }

    #[must_use]
    pub fn value(&self) -> (i64, &str) {
        (self.code, &self.message)
    }
}

#[derive(Clone, Copy, Debug)]
struct BuiltinRequestStateSlots {
    pcre: ExtensionStateSlot<PcreRequestState>,
    json: ExtensionStateSlot<JsonRequestState>,
    curl: ExtensionStateSlot<CurlState>,
}

#[derive(Debug)]
struct BuiltinRequestStateLayout {
    layout: ExtensionStateLayout,
    slots: BuiltinRequestStateSlots,
}

fn builtin_layout() -> &'static BuiltinRequestStateLayout {
    static LAYOUT: OnceLock<BuiltinRequestStateLayout> = OnceLock::new();
    LAYOUT.get_or_init(|| {
        let mut builder = ExtensionStateLayoutBuilder::new();
        let pcre = builder
            .register(PcreRequestState::default)
            .unwrap_or_else(|_| unreachable!("PCRE state is registered once"));
        let json = builder
            .register(JsonRequestState::default)
            .unwrap_or_else(|_| unreachable!("JSON state is registered once"));
        let curl = builder
            .register(CurlState::default)
            .unwrap_or_else(|_| unreachable!("cURL state is registered once"));
        BuiltinRequestStateLayout {
            layout: builder.build(),
            slots: BuiltinRequestStateSlots { pcre, json, curl },
        }
    })
}

/// Sole request owner for the migrated PCRE, JSON, and cURL states.
#[derive(Debug)]
pub struct BuiltinRequestState {
    state: RequestState,
}

impl Default for BuiltinRequestState {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinRequestState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: builtin_layout().layout.create_request_state(),
        }
    }

    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.state.slot_count()
    }

    #[must_use]
    pub fn payload_bytes() -> usize {
        builtin_layout().layout.payload_bytes()
    }

    pub fn pcre(&self) -> &PcreRequestState {
        self.state
            .get(builtin_layout().slots.pcre)
            .unwrap_or_else(|| unreachable!("request uses the builtin state layout"))
    }

    pub fn pcre_mut(&mut self) -> &mut PcreRequestState {
        self.state
            .get_mut(builtin_layout().slots.pcre)
            .unwrap_or_else(|| unreachable!("request uses the builtin state layout"))
    }

    pub fn json(&self) -> &JsonRequestState {
        self.state
            .get(builtin_layout().slots.json)
            .unwrap_or_else(|| unreachable!("request uses the builtin state layout"))
    }

    pub fn json_mut(&mut self) -> &mut JsonRequestState {
        self.state
            .get_mut(builtin_layout().slots.json)
            .unwrap_or_else(|| unreachable!("request uses the builtin state layout"))
    }

    pub fn curl(&self) -> &CurlState {
        self.state
            .get(builtin_layout().slots.curl)
            .unwrap_or_else(|| unreachable!("request uses the builtin state layout"))
    }

    pub fn curl_mut(&mut self) -> &mut CurlState {
        self.state
            .get_mut(builtin_layout().slots.curl)
            .unwrap_or_else(|| unreachable!("request uses the builtin state layout"))
    }

    /// Safe multi-state view used by builtins needing both parser states.
    pub fn pcre_and_json_mut(&mut self) -> (&mut PcreRequestState, &mut JsonRequestState) {
        self.state
            .get_pair_mut(builtin_layout().slots.pcre, builtin_layout().slots.json)
            .unwrap_or_else(|| unreachable!("PCRE and JSON use distinct registered slots"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_states_are_isolated_and_reset_on_new_request() {
        let mut first = BuiltinRequestState::new();
        first
            .json_mut()
            .set(super::super::context::JSON_ERROR_SYNTAX);
        first.pcre_mut().last_error_mut().set(2, "backtrack");

        let second = BuiltinRequestState::new();
        assert_eq!(
            second.json().value(),
            (super::super::context::JSON_ERROR_NONE, "No error")
        );
        assert_eq!(second.pcre().last_error().code(), 0);
    }

    #[test]
    fn full_builtin_layout_reports_three_migrated_payloads() {
        let state = BuiltinRequestState::new();
        assert_eq!(state.slot_count(), 3);
        assert_eq!(BuiltinRequestState::payload_bytes(), 200);
    }

    #[test]
    fn multiple_mutable_states_use_safe_pair_borrow() {
        let mut state = BuiltinRequestState::new();
        let (pcre, json) = state.pcre_and_json_mut();
        pcre.last_error_mut().set(3, "recursion");
        json.set(super::super::context::JSON_ERROR_RECURSION);
        assert_eq!(state.pcre().last_error().code(), 3);
        assert_eq!(
            state.json().value().0,
            super::super::context::JSON_ERROR_RECURSION
        );
    }
}
