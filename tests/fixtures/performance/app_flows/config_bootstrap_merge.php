<?php
function app_flow_scale() {
    $value = getenv('PHRUST_APP_FLOW_SCALE');
    $scale = (int)$value;
    if ($scale < 1) {
        return 1;
    }
    return $scale;
}

function app_flow_merge_config($defaults, $override) {
    foreach ($override as $key => $value) {
        if (isset($defaults[$key]) && is_array($defaults[$key]) && is_array($value)) {
            $section = $defaults[$key];
            foreach ($value as $innerKey => $innerValue) {
                $section[$innerKey] = $innerValue;
            }
            $defaults[$key] = $section;
        } else {
            $defaults[$key] = $value;
        }
    }
    return $defaults;
}

function app_flow_config_get($config, $path) {
    $parts = explode('.', $path);
    $value = $config;
    foreach ($parts as $part) {
        $value = $value[$part];
    }
    return $value;
}

$defaults = array(
    'app' => array('debug' => false, 'name' => 'phrust-app'),
    'cache' => array('enabled' => true, 'ttl' => 30),
    'routes' => array('home' => '/', 'api' => '/api'),
);
$override = array(
    'app' => array('debug' => '1'),
    'cache' => array('ttl' => '45'),
    'routes' => array('admin' => '/admin'),
);
$config = app_flow_merge_config($defaults, $override);
$config['app']['debug'] = (bool)$config['app']['debug'];
$config['cache']['ttl'] = (int)$config['cache']['ttl'];

$checksum = 0;
$items = 0;
$keys = array('app.name', 'app.debug', 'cache.enabled', 'cache.ttl', 'routes.home', 'routes.admin');
for ($round = 0; $round < app_flow_scale() * 30; $round++) {
    foreach ($keys as $key) {
        $value = app_flow_config_get($config, $key);
        if (is_bool($value)) {
            $checksum += $value ? 11 : 3;
        } else {
            $checksum += strlen((string)$value) + $round % 7;
        }
        $items++;
    }
}
echo 'app-flow config_bootstrap_merge checksum=' . $checksum . ' items=' . $items . "\n";
