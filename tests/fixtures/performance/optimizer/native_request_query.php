<?php

putenv('PHRUST_NATIVE_QUERY=ready');

$environment = getenv();
$nullableEnvironment = getenv(null);
var_dump(getenv('PHRUST_NATIVE_QUERY'));
var_dump($environment['PHRUST_NATIVE_QUERY'] ?? null);
var_dump($nullableEnvironment['PHRUST_NATIVE_QUERY'] ?? null);
var_dump(getenv('PHRUST_NATIVE_QUERY_MISSING'));

var_dump(is_string(sys_get_temp_dir()) && strlen(sys_get_temp_dir()) > 0);
var_dump(is_string(getcwd()) && strlen(getcwd()) > 0);
var_dump(php_sapi_name() === PHP_SAPI);
var_dump(is_string(get_current_user()) && strlen(get_current_user()) > 0);
var_dump(strlen(php_uname()) > 0);
var_dump(strlen(php_uname('s')) > 0);
var_dump(strlen(php_uname('n')) > 0);
var_dump(strlen(php_uname('r')) > 0);
var_dump(strlen(php_uname('v')) > 0);
var_dump(strlen(php_uname('m')) > 0);

$before = get_included_files();
include __DIR__ . '/native_request_query.inc';
$after = get_included_files();
var_dump($native_request_query_include_loaded);
var_dump(count($after) === count($before) + 1);
var_dump($after === get_required_files());
