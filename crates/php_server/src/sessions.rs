use super::state::AppState;
use crate::session_store::{generate_session_id, valid_session_id};
use php_executor::PhpExecutionOutput;
use php_runtime::api::{PHP_SESSION_ACTIVE, RuntimeHttpRequestContext, SessionState};
use std::sync::atomic::Ordering;

pub(crate) fn seed_session_state(
    request: &RuntimeHttpRequestContext,
    state: &AppState,
) -> Result<SessionState, String> {
    if !state.session_config.enabled {
        return Ok(SessionState::default());
    }
    state
        .metrics
        .session_seed_attempts
        .fetch_add(1, Ordering::Relaxed);
    let incoming_id = request
        .parsed_cookie
        .iter()
        .rev()
        .find(|(name, _)| name == &state.session_config.cookie_name)
        .map(|(_, value)| value.as_str())
        .filter(|value| valid_session_id(value))
        .unwrap_or("");
    let generated_id = generate_session_id().map_err(|error| {
        format!("E_PHP_SESSION_STORE_UNAVAILABLE: failed to generate session id: {error}")
    })?;
    Ok(SessionState::seeded_lazy(
        state.session_config.cookie_name.clone(),
        incoming_id.to_string(),
        Some(generated_id),
    ))
}

pub(crate) fn finalize_session_state(
    output: &mut PhpExecutionOutput,
    state: &AppState,
) -> Result<(), String> {
    if !state.session_config.enabled {
        return Ok(());
    }
    state
        .metrics
        .session_finalizations
        .fetch_add(1, Ordering::Relaxed);
    if output.session.destroyed() {
        if let Some(id) = output.session.destroyed_id() {
            state.session_store.delete(id).map_err(|error| {
                format!("E_PHP_SESSION_STORE_UNAVAILABLE: failed to delete session: {error}")
            })?;
            state
                .metrics
                .session_store_deletes
                .fetch_add(1, Ordering::Relaxed);
        }
        return Ok(());
    }
    if output.session.status() != PHP_SESSION_ACTIVE || output.session.id().is_empty() {
        state
            .metrics
            .session_finalize_skipped_inactive
            .fetch_add(1, Ordering::Relaxed);
        return Ok(());
    }
    state
        .session_store
        .save(output.session.id(), &output.session.data())
        .map_err(|error| {
            format!("E_PHP_SESSION_STORE_UNAVAILABLE: failed to save session: {error}")
        })?;
    state
        .metrics
        .session_store_writes
        .fetch_add(1, Ordering::Relaxed);
    if output.session.newly_created() {
        output
            .http_response
            .add_header_line(
                &format!(
                    "Set-Cookie: {}={}; Path={}; HttpOnly",
                    state.session_config.cookie_name,
                    output.session.id(),
                    state.session_config.cookie_path
                ),
                false,
                None,
            )
            .map_err(|message| format!("session cookie header failed: {message}"))?;
    }
    Ok(())
}
