//! Request-local HTTP, upload, session, and SAPI lifecycle state.

use super::prelude::*;

#[derive(Debug, Default)]
pub(super) struct RequestLifecycleState {
    pub(super) http_response: RuntimeHttpResponseState,
    pub(super) upload_registry: UploadRegistry,
    pub(super) session: php_runtime::SessionState,
    pub(super) session_loader: Option<php_runtime::SessionLoadCallback>,
    pub(super) sapi_name: String,
    pub(super) php_binary: String,
}

impl RequestLifecycleState {
    pub(super) fn from_runtime_context(context: &RuntimeContext) -> Self {
        Self {
            upload_registry: context.upload_registry(),
            session: context.session.clone(),
            session_loader: context.session_loader.clone(),
            sapi_name: context.sapi_name.clone(),
            php_binary: context.php_binary.clone(),
            ..Self::default()
        }
    }
}
