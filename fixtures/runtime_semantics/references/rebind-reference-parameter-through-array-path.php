<?php

require __DIR__ . '/../_data/rebind-reference-path-helper.php';

$referenced = 'original';
$container = array();
$container['value'] = &$referenced;
$copy = $container['value'];
$copy = 'copy';

var_dump($referenced, $copy);

function normalize_external_paths(array $input): array
{
    foreach (array(
        array('settings', 'color', 'palette'),
        array('settings', 'color', 'gradients'),
    ) as $path) {
        $preset = get_nested_value_external($input, $path);
        set_nested_value_external(
            $input,
            $path,
            array('default' => $preset),
        );
    }
    return $input;
}

$settings = array(
    'version' => 3,
    'settings' => array(
        'color' => array(
            'custom' => true,
            'palette' => array('black', 'white'),
            'gradients' => array('night', 'dawn'),
        ),
    ),
);

echo json_encode(normalize_external_paths($settings)), "\n";
echo json_encode($settings), "\n";
