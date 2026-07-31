<?php
// runtime-semantics: category=filesystem expect=pass
// Exact wrapper inventory, stream metadata, locality, and include resolution.

function nativeStreamQueryFamily(): array
{
    $stream = fopen(__FILE__, 'r');
    $metadata = stream_get_meta_data($stream);
    $previous = set_include_path(__DIR__);
    $resolved = stream_resolve_include_path(basename(__FILE__));
    set_include_path($previous);

    $wrappers = stream_get_wrappers();
    return [
        [in_array('file', $wrappers, true), in_array('php', $wrappers, true)],
        [
            $metadata['wrapper_type'],
            $metadata['stream_type'],
            $metadata['mode'],
            $metadata['uri'] === __FILE__,
            $metadata['seekable'],
            $metadata['eof'],
            $metadata['timed_out'],
            $metadata['blocked'],
        ],
        stream_is_local($stream),
        stream_is_local(__FILE__),
        stream_is_local('https://example.invalid/path'),
        $resolved === __FILE__,
        fclose($stream),
    ];
}

echo json_encode(nativeStreamQueryFamily()), "\n";
