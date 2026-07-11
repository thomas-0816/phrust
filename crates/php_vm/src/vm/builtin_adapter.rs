//! Request-local builtin adapter state and internal builtin dispatch caches.

use super::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InternalFunctionDispatchCacheOutcome {
    Hit,
    Miss,
    Uncached,
}

#[derive(Clone, Debug, Default)]
pub(super) struct InternalFunctionDispatchCache {
    entries: HashMap<String, BuiltinEntry>,
}

impl InternalFunctionDispatchCache {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(super) fn lookup(
        &mut self,
        name: &str,
    ) -> (Option<BuiltinEntry>, InternalFunctionDispatchCacheOutcome) {
        if !internal_function_dispatch_cacheable(name) {
            return (
                BuiltinRegistry::new().get(name),
                InternalFunctionDispatchCacheOutcome::Uncached,
            );
        }
        if let Some(entry) = self.entries.get(name).copied() {
            return (Some(entry), InternalFunctionDispatchCacheOutcome::Hit);
        }
        let entry = BuiltinRegistry::new().get(name);
        if let Some(entry) = entry {
            self.entries.insert(name.to_owned(), entry);
        }
        (entry, InternalFunctionDispatchCacheOutcome::Miss)
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct UserStreamWrapperRegistry {
    wrappers: BTreeMap<String, UserStreamWrapperClass>,
    open_streams: BTreeMap<php_runtime::ResourceId, UserStreamWrapperInstance>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct UserStreamWrapperClass {
    pub(super) protocol: String,
    pub(super) class_name: String,
    pub(super) display_class_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct UserStreamWrapperInstance {
    pub(super) object: ObjectRef,
    pub(super) close_called: bool,
}

impl UserStreamWrapperRegistry {
    pub(super) fn register(
        &mut self,
        protocol: &str,
        class_name: &str,
        display_class_name: &str,
    ) -> bool {
        let normalized = normalize_stream_wrapper_protocol(protocol);
        if normalized.is_empty() || self.wrappers.contains_key(&normalized) {
            return false;
        }
        self.wrappers.insert(
            normalized.clone(),
            UserStreamWrapperClass {
                protocol: normalized,
                class_name: class_name.to_owned(),
                display_class_name: display_class_name.to_owned(),
            },
        );
        true
    }

    pub(super) fn wrapper_for_uri(&self, uri: &str) -> Option<UserStreamWrapperClass> {
        let protocol = stream_uri_protocol(uri)?;
        self.wrappers.get(&protocol).cloned()
    }

    pub(super) fn protocols(&self) -> Vec<String> {
        self.wrappers
            .values()
            .map(|wrapper| wrapper.protocol.clone())
            .collect()
    }

    pub(super) fn register_open_stream(
        &mut self,
        resource: &php_runtime::ResourceRef,
        object: ObjectRef,
    ) {
        self.open_streams.insert(
            resource.id(),
            UserStreamWrapperInstance {
                object,
                close_called: false,
            },
        );
    }

    pub(super) fn pending_close_object(
        &mut self,
        id: php_runtime::ResourceId,
    ) -> Option<ObjectRef> {
        let instance = self.open_streams.get_mut(&id)?;
        if instance.close_called {
            return None;
        }
        instance.close_called = true;
        Some(instance.object.clone())
    }

    pub(super) fn pending_close_ids(&self) -> Vec<php_runtime::ResourceId> {
        self.open_streams
            .iter()
            .filter_map(|(id, instance)| (!instance.close_called).then_some(*id))
            .collect()
    }
}

#[derive(Debug, Default)]
pub(super) struct BuiltinAdapterState {
    pub(super) bcmath_scale: usize,
    pub(super) strtok_state: php_runtime::StrtokState,
    pub(super) iconv_state: php_runtime::IconvEncodingState,
    pub(super) apcu_state: php_runtime::ApcuState,
    pub(super) opcache_state: php_runtime::OpcacheState,
    pub(super) soap_state: php_runtime::SoapState,
    pub(super) openssl_error_state: php_runtime::OpenSslErrorState,
    pub(super) gettext_state: php_runtime::GettextState,
    pub(super) shmop_state: php_runtime::ShmopState,
    pub(super) readline_state: php_runtime::ReadlineState,
    pub(super) sysvmsg_state: php_runtime::SysvMessageQueueState,
    pub(super) sysvsem_state: php_runtime::SysvSemaphoreState,
    pub(super) sysvshm_state: php_runtime::SysvSharedMemoryState,
    pub(super) pcntl_state: php_runtime::PcntlState,
    pub(super) ftp_state: php_runtime::FtpState,
    pub(super) imap_state: php_runtime::ImapState,
    pub(super) ldap_state: php_runtime::LdapState,
    pub(super) ssh2_state: php_runtime::Ssh2State,
    pub(super) socket_state: php_runtime::SocketState,
    pub(super) filesystem_state: php_runtime::FilesystemRuntimeState,
    pub(super) stream_context_state: php_runtime::StreamContextState,
    pub(super) user_stream_wrappers: UserStreamWrapperRegistry,
    pub(super) mb_internal_encoding: String,
    pub(super) mb_substitute_character: php_runtime::MbSubstituteCharacter,
    pub(super) builtin_request_state: php_runtime::BuiltinRequestState,
    pub(super) json_serializable_active_objects: Vec<u64>,
    pub(super) posix_last_error: i32,
    pub(super) sqlite: php_runtime::SqliteState,
    pub(super) mysql: php_runtime::MysqlState,
    pub(super) postgres: php_runtime::PostgresState,
    pub(super) redis_clients: RedisClientState,
    pub(super) memcached_clients: MemcachedClientState,
}

impl BuiltinAdapterState {
    pub(super) fn pcre_state_mut(&mut self) -> &mut php_runtime::PcreRequestState {
        self.builtin_request_state.pcre_mut()
    }

    pub(super) fn set_json_last_error(&mut self, code: i64) {
        self.builtin_request_state.json_mut().set(code);
    }
}
