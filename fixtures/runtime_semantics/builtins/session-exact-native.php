<?php
// runtime-semantics: requires_ref_extension=session php_ref_optional_reason=reference-build-lacks-session

function compile_exact_session(object $handler): array
{
    return [
        session_abort(),
        session_cache_expire(),
        session_cache_limiter(),
        session_commit(),
        session_destroy(),
        session_gc(),
        session_decode(''),
        session_encode(),
        session_create_id(),
        session_get_cookie_params(),
        session_id(),
        session_module_name(),
        session_name(),
        session_regenerate_id(),
        session_register_shutdown(),
        session_reset(),
        session_save_path(),
        session_set_cookie_params(0),
        session_set_save_handler($handler),
        session_start(),
        session_status(),
        session_unset(),
        session_write_close(),
    ];
}

function run_exact_session_configuration(): array
{
    session_name('APPSESSID');
    session_id('request-id');
    session_cache_expire(60);
    session_cache_limiter('private');
    session_save_path('');
    session_module_name('files');
    session_set_cookie_params(3600, '/app', 'example.test', true, true);
    $cookie = session_get_cookie_params();

    return [
        session_status(),
        session_name(),
        session_id(),
        session_cache_expire(),
        session_cache_limiter(),
        session_save_path(),
        session_module_name(),
        $cookie['lifetime'],
        $cookie['path'],
        $cookie['domain'],
        $cookie['secure'],
        $cookie['httponly'],
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_exact_session_configuration();
}
var_dump($result);
