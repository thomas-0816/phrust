use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

pub(super) type NativeTraceArguments = smallvec::SmallVec<[i64; 4]>;

#[derive(Clone)]
pub(super) struct NativeBacktraceFrame {
    /// One shared metadata record replaces separate function/file/class
    /// refcount bumps on every userland call. Introspection materializes the
    /// individual fields only when PHP actually requests a backtrace.
    pub(super) metadata: Option<super::NativeFunctionMetadataPtr>,
    pub(super) class: Option<Arc<str>>,
    pub(super) object: Option<php_runtime::api::ObjectRef>,
    /// Arguments are present on every native PHP call for `func_get_arg()` and
    /// backtraces. Preserve their already-live arena handles inline instead of
    /// cloning PHP values on every call; exceptional/introspection paths
    /// materialize values lazily while the synchronous caller still owns them.
    pub(super) arguments: NativeTraceArguments,
    /// Leading visible arguments backed by mutable fixed parameter locals.
    pub(super) fixed_argument_count: u32,
}

/// Persistent, structurally shared function visibility for nested PHP units.
///
/// Includes usually add a small set of names to a much larger inherited set.
/// Keeping parent scopes avoids cloning the complete request symbol table for
/// every include while preserving PHP's request-wide function visibility.
#[derive(Default)]
pub(crate) struct NativeFunctionNameScope {
    parent: Option<Rc<Self>>,
    names: BTreeSet<String>,
}

impl NativeFunctionNameScope {
    pub(super) fn child(parent: Rc<Self>, names: impl IntoIterator<Item = String>) -> Rc<Self> {
        let names = names.into_iter().collect::<BTreeSet<_>>();
        if names.is_empty() {
            parent
        } else {
            Rc::new(Self {
                parent: Some(parent),
                names,
            })
        }
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        let mut scope = Some(self);
        while let Some(current) = scope {
            if current.names.contains(name) {
                return true;
            }
            scope = current.parent.as_deref();
        }
        false
    }
}

#[derive(Clone)]
pub(crate) struct NativeLastError {
    pub(crate) error_type: i64,
    pub(crate) message: String,
    pub(crate) file: String,
    pub(crate) line: usize,
}

#[derive(Debug)]
pub(super) struct NativeRegisteredExtensionRequestState {
    state: php_runtime::api::RequestState,
    apcu: php_runtime::api::ExtensionStateSlot<php_runtime::api::ApcuState>,
    strtok: php_runtime::api::StrtokState,
    iconv: php_runtime::api::IconvEncodingState,
    opcache: php_runtime::api::OpcacheState,
    soap: php_runtime::api::SoapState,
    openssl: php_runtime::api::OpenSslErrorState,
    gettext: php_runtime::api::GettextState,
    shmop: php_runtime::api::ShmopState,
    readline: php_runtime::api::ReadlineState,
    sysvmsg: php_runtime::api::SysvMessageQueueState,
    sysvsem: php_runtime::api::SysvSemaphoreState,
    sysvshm: php_runtime::api::SysvSharedMemoryState,
    pcntl: php_runtime::api::PcntlState,
    ftp: php_runtime::api::FtpState,
    imap: php_runtime::api::ImapState,
    ldap: php_runtime::api::LdapState,
    ssh2: php_runtime::api::Ssh2State,
    sockets: php_runtime::api::SocketState,
    filesystem: php_runtime::api::FilesystemRuntimeState,
    pub(super) stream_context: php_runtime::api::StreamContextState,
    bcmath_scale: usize,
    mb_internal_encoding: String,
    mb_substitute_character: php_runtime::api::MbSubstituteCharacter,
    postgres: php_runtime::api::PostgresState,
}

impl Default for NativeRegisteredExtensionRequestState {
    fn default() -> Self {
        let registry = php_extensions::BuiltinRegistry::new();
        let apcu = registry
            .request_state_slot("apcu")
            .unwrap_or_else(|| unreachable!("default registry selects APCu"));
        Self {
            state: registry.create_request_state(),
            apcu,
            strtok: Default::default(),
            iconv: Default::default(),
            opcache: Default::default(),
            soap: Default::default(),
            openssl: Default::default(),
            gettext: Default::default(),
            shmop: Default::default(),
            readline: Default::default(),
            sysvmsg: Default::default(),
            sysvsem: Default::default(),
            sysvshm: Default::default(),
            pcntl: Default::default(),
            ftp: Default::default(),
            imap: Default::default(),
            ldap: Default::default(),
            ssh2: Default::default(),
            sockets: Default::default(),
            filesystem: Default::default(),
            stream_context: Default::default(),
            bcmath_scale: 0,
            mb_internal_encoding: "UTF-8".to_owned(),
            mb_substitute_character: Default::default(),
            postgres: Default::default(),
        }
    }
}

impl NativeRegisteredExtensionRequestState {
    pub(super) const fn is_fork_child(&self) -> bool {
        self.pcntl.is_fork_child()
    }

    pub(super) fn mb_internal_encoding_ptr(&mut self) -> *mut String {
        std::ptr::from_mut(&mut self.mb_internal_encoding)
    }

    pub(super) fn bcmath_scale_ptr(&mut self) -> *mut usize {
        std::ptr::from_mut(&mut self.bcmath_scale)
    }

    pub(super) fn mb_substitute_character_ptr(
        &mut self,
    ) -> *mut php_runtime::api::MbSubstituteCharacter {
        std::ptr::from_mut(&mut self.mb_substitute_character)
    }

    pub(super) fn filesystem_ptr(&mut self) -> *mut php_runtime::api::FilesystemRuntimeState {
        std::ptr::from_mut(&mut self.filesystem)
    }
}

#[cfg(test)]
mod tests {
    use super::NativeFunctionNameScope;
    use std::rc::Rc;

    #[test]
    fn function_name_scopes_share_inherited_names() {
        let root = NativeFunctionNameScope::child(
            Rc::new(NativeFunctionNameScope::default()),
            ["root_fn".to_owned()],
        );
        let child = NativeFunctionNameScope::child(root.clone(), ["child_fn".to_owned()]);

        assert!(child.contains("root_fn"));
        assert!(child.contains("child_fn"));
        assert!(!child.contains("missing_fn"));
        assert!(Rc::ptr_eq(
            child.parent.as_ref().expect("child scope parent"),
            &root
        ));
    }
}
