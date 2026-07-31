<?php
// runtime-semantics: category=wordpress_blockers expect=pass

function run_exact_request_queries(): array
{
    $environment = getenv();

    return [
        getcwd(),
        sys_get_temp_dir(),
        php_sapi_name(),
        getenv('LC_ALL'),
        getenv('PHRUST_MISSING_ENVIRONMENT_VALUE'),
        is_array($environment),
        $environment['LC_ALL'] ?? null,
        array_key_exists('PHRUST_MISSING_ENVIRONMENT_VALUE', $environment),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_exact_request_queries();
}
var_dump($result);
