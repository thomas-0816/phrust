<?php
// runtime-semantics: expect=pass
function compact_values(): array {
    $charset = 'utf8mb4';
    $collate = '';
    return compact('charset', ['collate']);
}

$values = compact_values();
echo $values['charset'], '|';
var_dump($values['collate']);

function compact_values_are_copied(): void {
    $postarr = compact_parse_args(
        array('post_type' => 'wp_navigation'),
        array('post_status' => 'publish')
    );
    foreach (array_keys($postarr) as $field) {
        $postarr[$field] = compact_identity($postarr[$field]);
    }
    $post_type = $postarr['post_type'];
    $data = compact('post_type');
    $processed = compact_wrap_values($data);
    var_dump($post_type, $data['post_type'], $processed['post_type']);
}

function compact_parse_args(array $args, array $defaults): array {
    $parsed_args =& $args;
    return array_merge($defaults, $parsed_args);
}

function compact_identity($value) {
    return $value;
}

function compact_wrap_values(array $data): array {
    foreach ($data as $field => $value) {
        $data[$field] = array('value' => $value);
    }
    return $data;
}

compact_values_are_copied();

function compact_dereferences_source_values(): void {
    $source = 'original';
    $alias =& $source;
    $values = compact('source');
    $values['source'] = 'copy';
    var_dump($source, $alias, $values['source']);
}

compact_dereferences_source_values();
