<?php

function external_compact_cow(): void
{
    $postarr = array('post_type' => 'wp_navigation');
    $post_type = $postarr['post_type'];

    echo external_compact_by_value($post_type), '|';
    echo external_compact_by_value($post_type, 'pingback'), '|';

    $data = compact('post_type');
    $data['post_type'] = 'changed';
    $processed = (new ExternalCompactProcessor())->insert('posts', $data);
    var_dump($post_type, $data['post_type'], $processed['post_type']);
}
