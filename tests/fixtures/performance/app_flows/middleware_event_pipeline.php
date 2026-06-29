<?php
function app_flow_scale() {
    $value = getenv('PHRUST_APP_FLOW_SCALE');
    $scale = (int)$value;
    if ($scale < 1) {
        return 1;
    }
    return $scale;
}

function app_flow_dispatch_event(&$events, $name, $payload) {
    $amount = strlen($payload);
    if ($name === 'start') {
        $events['start'] = $events['start'] + $amount;
    } elseif ($name === 'auth') {
        $events['auth'] = $events['auth'] + $amount;
    } elseif ($name === 'controller') {
        $events['controller'] = $events['controller'] + $amount;
    } else {
        $events['finish'] = $events['finish'] + $amount;
    }
}

function app_flow_controller($request, &$events) {
    app_flow_dispatch_event($events, 'controller', $request['path']);
    return array('status' => 200, 'body' => 'ok:' . $request['path']);
}

function app_flow_pipeline($request, &$events) {
    app_flow_dispatch_event($events, 'start', $request['method']);
    $trace = 'trace-' . strlen($request['path']);
    app_flow_dispatch_event($events, 'auth', $request['headers']['role']);
    $response = app_flow_controller($request, $events);
    $response['body'] = $response['body'] . ':mw';
    app_flow_dispatch_event($events, 'finish', $response['body']);
    return $response;
}

$requests = array(
    array('method' => 'GET', 'path' => '/users', 'headers' => array('role' => 'admin')),
    array('method' => 'POST', 'path' => '/orders', 'headers' => array('role' => 'editor')),
);
$checksum = 0;
$items = 0;
for ($round = 0; $round < app_flow_scale() * 30; $round++) {
    $events = array('start' => 0, 'auth' => 0, 'controller' => 0, 'finish' => 0);
    foreach ($requests as $request) {
        $response = app_flow_pipeline($request, $events);
        $rowChecksum = $response['status'] + strlen($response['body']) + count($events) + $round % 4;
        $checksum = $checksum + $rowChecksum;
        $items++;
    }
}
echo 'app-flow middleware_event_pipeline checksum=' . $checksum . ' items=' . $items . "\n";
