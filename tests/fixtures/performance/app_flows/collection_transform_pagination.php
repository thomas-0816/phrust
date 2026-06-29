<?php
function app_flow_scale() {
    $value = getenv('PHRUST_APP_FLOW_SCALE');
    $scale = (int)$value;
    if ($scale < 1) {
        return 1;
    }
    return $scale;
}

function app_flow_sort_records($left, $right) {
    if ($left['score'] === $right['score']) {
        return $left['id'] <=> $right['id'];
    }
    return $right['score'] <=> $left['score'];
}

$records = array();
for ($i = 0; $i < 36; $i++) {
    if ($i % 3 === 0) {
        $group = 'alpha';
    } elseif ($i % 3 === 1) {
        $group = 'beta';
    } else {
        $group = 'gamma';
    }
    $id = $i + 1;
    $score = ($i * 17) % 101;
    if ($i % 4 !== 0) {
        $active = true;
    } else {
        $active = false;
    }
    $record = array();
    $record['id'] = $id;
    $record['group'] = $group;
    $record['score'] = $score;
    $record['active'] = $active;
    $records[] = $record;
}

$checksum = 0;
$items = 0;
for ($round = 0; $round < app_flow_scale() * 10; $round++) {
    $active = array();
    foreach ($records as $record) {
        if ($record['active']) {
            $record['score'] = $record['score'] + ($round % 5);
            $active[] = $record;
        }
    }
    usort($active, 'app_flow_sort_records');
    $page = array_slice($active, 3, 8);
    $groups = array();
    foreach ($page as $record) {
        $group = $record['group'];
        if (!isset($groups[$group])) {
            $groups[$group] = 0;
        }
        $groupTotal = $groups[$group] + $record['score'];
        $groups[$group] = $groupTotal;
        $rowChecksum = $record['id'] + $record['score'] + strlen($group);
        $checksum = $checksum + $rowChecksum;
        $items++;
    }
    $checksum = $checksum + count($groups);
}
echo 'app-flow collection_transform_pagination checksum=' . $checksum . ' items=' . $items . "\n";
