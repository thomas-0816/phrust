<?php
// runtime-semantics: category=strings expect=pass

function run_native_rewrite_html(string $input): array
{
    return [
        addcslashes($input, "\0..\37!\\"),
        ucwords('alpha-beta gamma', " -"),
        str_pad('native', 13, '.-', STR_PAD_BOTH),
        str_pad('already-long', 4, 'x', STR_PAD_LEFT),
        strtr('abracadabra', 'abc', 'XYZ'),
        substr_replace('abcdefgh', 'NATIVE', -5, 3),
        substr_replace('abcdefgh', '', 2, -2),
        htmlspecialchars("<a \"x\">&amp; 'y'</a>", ENT_QUOTES, 'UTF-8', false),
        htmlentities("© € <& '", ENT_QUOTES | ENT_HTML5, 'UTF-8', true),
        html_entity_decode('&lt;&#x20ac;&amp;&quot;&#039;', ENT_QUOTES | ENT_HTML5),
        htmlspecialchars_decode('&lt;&#169;&quot;&#039;&amp;', ENT_QUOTES | ENT_HTML5),
        strip_tags('<b>keep</b><i>drop</i>', '<b>'),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_native_rewrite_html("native\0line\n!\xff");
}

var_dump($result);
