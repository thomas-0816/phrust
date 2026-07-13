//! Capability-limited views used behind the generic builtin dispatch ABI.

use super::*;
use crate::builtins::request_state::{JsonRequestState, PcreRequestState};
use crate::builtins::{BuiltinEntry, BuiltinResult};

/// JSON builtins can mutate only JSON's request-local diagnostic state.
pub(in crate::builtins) struct JsonBuiltinServices<'a> {
    state: &'a mut JsonRequestState,
}

impl JsonBuiltinServices<'_> {
    pub fn set_json_last_error(&mut self, code: i64) {
        self.state.set(code);
    }

    #[must_use]
    pub fn json_last_error(&self) -> (i64, &str) {
        self.state.value()
    }
}

/// PCRE's request state plus the two core services used by pattern compilation.
pub(in crate::builtins) struct PcreBuiltinServices<'context, 'output> {
    state: &'context mut PcreRequestState,
    ini: &'context IniRegistry,
    io: &'context mut BuiltinIoContext<'output>,
}

pub(in crate::builtins) trait PcreServiceAccess {
    fn pcre_cache(&mut self) -> &mut PcreCache;
    fn set_preg_last_error(&mut self, code: i64, message: impl Into<String>);
    fn clear_preg_last_error(&mut self);
    fn preg_last_error(&self) -> (i64, &str);
    fn ini_get(&self, name: &str) -> Option<&str>;
    fn string_cast_value(
        &mut self,
        value: &Value,
        span: RuntimeSourceSpan,
    ) -> Result<crate::PhpString, String>;
    fn php_warning(
        &mut self,
        id: impl Into<String>,
        message: impl Into<String>,
        span: RuntimeSourceSpan,
    );
}

pub(in crate::builtins) trait PcreCallbackServiceAccess:
    PcreServiceAccess
{
    fn invoke_builtin(
        &mut self,
        callback: BuiltinEntry,
        args: Vec<Value>,
        span: RuntimeSourceSpan,
    ) -> BuiltinResult;
}

impl PcreServiceAccess for PcreBuiltinServices<'_, '_> {
    fn pcre_cache(&mut self) -> &mut PcreCache {
        self.state.cache_mut()
    }

    fn set_preg_last_error(&mut self, code: i64, message: impl Into<String>) {
        self.state.last_error_mut().set(code, message);
    }

    fn clear_preg_last_error(&mut self) {
        self.state.last_error_mut().clear();
    }

    fn preg_last_error(&self) -> (i64, &str) {
        let state = self.state.last_error();
        (state.code(), state.message())
    }

    fn ini_get(&self, name: &str) -> Option<&str> {
        self.ini.get(name)
    }

    fn string_cast_value(
        &mut self,
        value: &Value,
        span: RuntimeSourceSpan,
    ) -> Result<crate::PhpString, String> {
        pcre_string_cast_value(self, value, span)
    }

    fn php_warning(
        &mut self,
        id: impl Into<String>,
        message: impl Into<String>,
        span: RuntimeSourceSpan,
    ) {
        self.io.php_warning(id, message, span);
    }
}

/// PCRE callback adapter exposes only PCRE operations and explicit callback invocation.
pub(in crate::builtins) struct PcreCallbackServices<'context, 'output> {
    context: &'context mut BuiltinContext<'output>,
}

impl PcreServiceAccess for PcreCallbackServices<'_, '_> {
    fn pcre_cache(&mut self) -> &mut PcreCache {
        self.context.request_state.get_mut().pcre_mut().cache_mut()
    }

    fn set_preg_last_error(&mut self, code: i64, message: impl Into<String>) {
        self.context
            .request_state
            .get_mut()
            .pcre_mut()
            .last_error_mut()
            .set(code, message);
    }

    fn clear_preg_last_error(&mut self) {
        self.context
            .request_state
            .get_mut()
            .pcre_mut()
            .last_error_mut()
            .clear();
    }

    fn preg_last_error(&self) -> (i64, &str) {
        let state = self.context.request_state.get().pcre().last_error();
        (state.code(), state.message())
    }

    fn ini_get(&self, name: &str) -> Option<&str> {
        self.context.ini_get(name)
    }

    fn string_cast_value(
        &mut self,
        value: &Value,
        span: RuntimeSourceSpan,
    ) -> Result<crate::PhpString, String> {
        pcre_string_cast_value(self, value, span)
    }

    fn php_warning(
        &mut self,
        id: impl Into<String>,
        message: impl Into<String>,
        span: RuntimeSourceSpan,
    ) {
        self.context.php_warning(id, message, span);
    }
}

impl PcreCallbackServiceAccess for PcreCallbackServices<'_, '_> {
    fn invoke_builtin(
        &mut self,
        callback: BuiltinEntry,
        args: Vec<Value>,
        span: RuntimeSourceSpan,
    ) -> BuiltinResult {
        (callback.function())(self.context, args, span)
    }
}

fn pcre_string_cast_value<S: PcreServiceAccess>(
    services: &mut S,
    value: &Value,
    span: RuntimeSourceSpan,
) -> Result<crate::PhpString, String> {
    match value {
        Value::Array(_) => {
            services.php_warning(
                "E_PHP_RUNTIME_ARRAY_TO_STRING_WARNING",
                "Array to string conversion",
                span,
            );
            Ok(crate::PhpString::from_test_str("Array"))
        }
        Value::Object(object)
            if crate::normalize_class_name(&object.class_name()) == "phptoken" =>
        {
            match object.get_property("text") {
                Some(Value::String(text)) => Ok(text),
                _ => crate::to_string(value),
            }
        }
        Value::Reference(cell) => pcre_string_cast_value(services, &cell.get(), span),
        other => crate::to_string(other),
    }
}

/// cURL's request state, output, diagnostics, and explicit network capability.
pub(in crate::builtins) struct CurlBuiltinServices<'context, 'output> {
    state: &'context mut CurlState,
    io: &'context mut BuiltinIoContext<'output>,
    network_requests_enabled: bool,
}

impl CurlBuiltinServices<'_, '_> {
    pub fn curl_state(&mut self) -> &mut CurlState {
        self.state
    }

    #[must_use]
    pub fn curl_state_ref(&self) -> &CurlState {
        self.state
    }

    #[must_use]
    pub const fn network_requests_enabled(&self) -> bool {
        self.network_requests_enabled
    }

    pub fn output(&mut self) -> &mut OutputBuffer {
        self.io.output
    }

    pub fn record_diagnostic(&mut self, diagnostic: RuntimeDiagnostic) {
        self.io.diagnostics.push(diagnostic);
    }
}

impl<'output> BuiltinContext<'output> {
    pub(in crate::builtins) fn json_services(&mut self) -> JsonBuiltinServices<'_> {
        JsonBuiltinServices {
            state: self.request_state.get_mut().json_mut(),
        }
    }

    pub(in crate::builtins) fn pcre_services(&mut self) -> PcreBuiltinServices<'_, 'output> {
        let ini = match self.ini_slot.as_deref() {
            Some(ini) => ini,
            None => &self.ini,
        };
        PcreBuiltinServices {
            state: self.request_state.get_mut().pcre_mut(),
            ini,
            io: &mut self.io,
        }
    }

    pub(in crate::builtins) fn pcre_callback_services(
        &mut self,
    ) -> PcreCallbackServices<'_, 'output> {
        PcreCallbackServices { context: self }
    }

    pub(in crate::builtins) fn curl_services(&mut self) -> CurlBuiltinServices<'_, 'output> {
        CurlBuiltinServices {
            state: self.request_state.get_mut().curl_mut(),
            io: &mut self.io,
            network_requests_enabled: self.network_requests_enabled,
        }
    }
}
