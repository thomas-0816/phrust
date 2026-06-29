<?php
function app_flow_scale() {
    $value = getenv('PHRUST_APP_FLOW_SCALE');
    $scale = (int)$value;
    if ($scale < 1) {
        return 1;
    }
    return $scale;
}

function app_flow_render_card($item) {
    $html = '<article>';
    $html .= '<h2>' . htmlspecialchars($item['title']) . '</h2>';
    if ($item['active']) {
        $html .= '<span>active</span>';
    } else {
        $html .= '<span>draft</span>';
    }
    $html .= '<ul>';
    foreach ($item['tags'] as $tag) {
        $html .= '<li>' . htmlspecialchars($tag) . '</li>';
    }
    $html .= '</ul>';
    $html .= '</article>';
    return $html;
}

$items = array(
    array('title' => 'Alpha & One', 'active' => true, 'tags' => array('red', 'blue')),
    array('title' => 'Beta <Two>', 'active' => false, 'tags' => array('green', 'white')),
    array('title' => 'Gamma "Three"', 'active' => true, 'tags' => array('black', 'orange')),
);
$checksum = 0;
$count = 0;
for ($round = 0; $round < app_flow_scale() * 25; $round++) {
    ob_start();
    foreach ($items as $item) {
        echo app_flow_render_card($item);
        $count++;
    }
    $rendered = ob_get_clean();
    $checksum += strlen($rendered) + $round;
}
echo 'app-flow template_render_escape checksum=' . $checksum . ' items=' . $count . "\n";
