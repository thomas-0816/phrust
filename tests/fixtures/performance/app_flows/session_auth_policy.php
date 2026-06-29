<?php
function app_flow_scale() {
    $value = getenv('PHRUST_APP_FLOW_SCALE');
    $scale = (int)$value;
    if ($scale < 1) {
        return 1;
    }
    return $scale;
}

function app_flow_can_access($session, $resource, &$cache) {
    $key = $session['user_id'] . ':' . $resource['action'];
    if (isset($cache[$key])) {
        return $cache[$key];
    }
    $allowed = false;
    foreach ($session['roles'] as $role) {
        if (isset($resource['permissions'][$role]) && $resource['permissions'][$role]) {
            $allowed = true;
        }
    }
    if ($session['csrf'] !== $resource['csrf']) {
        $allowed = false;
    }
    $cache[$key] = $allowed;
    return $allowed;
}

$sessions = array(
    array('user_id' => 7, 'roles' => array('admin'), 'csrf' => 'tok-a'),
    array('user_id' => 8, 'roles' => array('viewer'), 'csrf' => 'tok-b'),
    array('user_id' => 9, 'roles' => array('editor'), 'csrf' => 'bad'),
);
$resources = array(
    array('action' => 'edit', 'csrf' => 'tok-a', 'permissions' => array('admin' => true, 'editor' => true)),
    array('action' => 'view', 'csrf' => 'tok-b', 'permissions' => array('viewer' => true, 'admin' => true)),
    array('action' => 'delete', 'csrf' => 'tok-c', 'permissions' => array('admin' => true)),
);
$checksum = 0;
$items = 0;
for ($round = 0; $round < app_flow_scale() * 25; $round++) {
    $cache = array();
    foreach ($sessions as $session) {
        foreach ($resources as $resource) {
            $allowed = app_flow_can_access($session, $resource, $cache);
            $status = $allowed ? 200 : 403;
            $checksum += $status + count($cache) + $round % 6;
            $items++;
        }
    }
}
echo 'app-flow session_auth_policy checksum=' . $checksum . ' items=' . $items . "\n";
