<?php
function app_flow_scale() {
    $value = getenv('PHRUST_APP_FLOW_SCALE');
    $scale = (int)$value;
    if ($scale < 1) {
        return 1;
    }
    return $scale;
}

function app_flow_translate($catalog, $locale, $key, $params) {
    if ($key === 'status') {
        return 'Status: ' . $params['status'];
    }
    if ($locale === 'de') {
        return $params['name'] . ' hat ' . $params['count'] . ' Artikel';
    }
    if ($key === 'cart.one') {
        return $params['name'] . ' has ' . $params['count'] . ' item';
    }
    return $params['name'] . ' has ' . $params['count'] . ' items';
}

$catalog = array(
    'en' => array(
        'cart.one' => '{name} has {count} item',
        'cart.many' => '{name} has {count} items',
        'status' => 'Status: {status}',
    ),
    'de' => array(
        'cart.one' => '{name} hat {count} Artikel',
        'cart.many' => '{name} hat {count} Artikel',
    ),
);
$requests = array(
    array('locale' => 'en', 'name' => 'Ada', 'count' => 1, 'status' => 'ready'),
    array('locale' => 'de', 'name' => 'Lin', 'count' => 3, 'status' => 'bereit'),
    array('locale' => 'fr', 'name' => 'Max', 'count' => 2, 'status' => 'pret'),
);
$checksum = 0;
$items = 0;
for ($round = 0; $round < app_flow_scale() * 30; $round++) {
    foreach ($requests as $request) {
        if ($request['count'] === 1) {
            $key = 'cart.one';
        } else {
            $key = 'cart.many';
        }
        $line = app_flow_translate($catalog, $request['locale'], $key, array('name' => $request['name'], 'count' => $request['count']));
        $status = app_flow_translate($catalog, $request['locale'], 'status', array('status' => $request['status']));
        $checksum = $checksum + strlen($line) + strlen($status) + $round % 8;
        $items++;
    }
}
echo 'app-flow translation_lookup_interpolation checksum=' . $checksum . ' items=' . $items . "\n";
