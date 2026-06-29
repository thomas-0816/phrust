<?php
function app_flow_scale() {
    $value = getenv('PHRUST_APP_FLOW_SCALE');
    $scale = (int)$value;
    if ($scale < 1) {
        return 1;
    }
    return $scale;
}

class AppFlowRouterController {
    public function detail($params) {
        return 'detail:' . $params['id'];
    }

    public function archive($params) {
        return 'archive:' . $params['year'] . ':' . $params['slug'];
    }
}

function app_flow_route_match($route, $request) {
    if ($route['method'] !== $request['method']) {
        return null;
    }
    if ($route['handler'] === 'home') {
        if ($request['path'] === '/') {
            return array();
        }
        return null;
    }
    if ($route['handler'] === 'detail') {
        if (substr($request['path'], 0, 7) !== '/items/') {
            return null;
        }
        $params = array();
        $params['id'] = substr($request['path'], 7);
        return $params;
    }
    if (substr($request['path'], 0, 9) !== '/archive/') {
        return null;
    }
    $rest = substr($request['path'], 9);
    $parts = explode('/', $rest);
    if (count($parts) !== 2) {
        return null;
    }
    $params = array();
    $params['year'] = $parts[0];
    $params['slug'] = $parts[1];
    return $params;
}

function app_flow_front_controller($request, $routes, $controller) {
    foreach ($routes as $route) {
        $params = app_flow_route_match($route, $request);
        if ($params !== null) {
            $handler = $route['handler'];
            if ($handler === 'home') {
                return 'home:index';
            }
            if ($handler === 'detail') {
                return $controller->detail($params);
            }
            return $controller->archive($params);
        }
    }
    return 'not-found';
}

$routes = array(
    array('method' => 'GET', 'path' => '/', 'type' => 'static', 'handler' => 'home'),
    array('method' => 'GET', 'path' => '/items/:id', 'type' => 'pattern', 'handler' => 'detail'),
    array('method' => 'GET', 'path' => '/archive/:year/:slug', 'type' => 'pattern', 'handler' => 'archive'),
);
$requests = array(
    array('method' => 'GET', 'path' => '/'),
    array('method' => 'GET', 'path' => '/items/42'),
    array('method' => 'GET', 'path' => '/archive/2026/release'),
    array('method' => 'POST', 'path' => '/items/42'),
);
$controller = new AppFlowRouterController();
$checksum = 0;
$items = 0;
for ($round = 0; $round < app_flow_scale() * 20; $round++) {
    foreach ($requests as $request) {
        $result = app_flow_front_controller($request, $routes, $controller);
        $checksum = $checksum + strlen($result) + $round + count($routes);
        $items++;
    }
}
echo 'app-flow front_controller_routing checksum=' . $checksum . ' items=' . $items . "\n";
