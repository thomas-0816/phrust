<?php
// runtime-semantics: category=include_eval_autoload expect=pass
require __DIR__ . '/_data/external_parent_property/ParentHooks.php';
require __DIR__ . '/_data/external_parent_property/ChildHooks.php';
require __DIR__ . '/_data/external_parent_property/CookieJar.php';
require __DIR__ . '/_data/external_parent_property/Cache.php';

function fixture_action_ref_array(string $name, array $parameters): void
{
}

$hooks = new ChildHooks('https://example.test', ['timeout' => 5]);
$jar = new CookieJar();
var_dump($jar->optionalReference());
echo fixture_cache_get('cache-key'), "\n";
$hooks->register('before_request', [$jar, 'beforeRequest']);
$url = 'https://example.test';
$headers = [];
$data = null;
$type = 'GET';
$options = ['timeout' => 5];
var_dump($hooks->dispatch('before_request', [&$url, &$headers, &$data, &$type, &$options]));
echo $headers['cookie'], '|', $options['seen'], "\n";
