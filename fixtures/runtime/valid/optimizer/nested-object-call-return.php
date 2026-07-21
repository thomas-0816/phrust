<?php

class OptimizerNestedTranslator {
    public function translate($value, $context = null) {
        return $value;
    }
}

function optimizer_nested_translator() {
    global $optimizer_translations;
    if (isset($optimizer_translations['default'])) {
        return $optimizer_translations['default'];
    }
    static $translator = null;
    if (null === $translator) {
        $translator = new OptimizerNestedTranslator();
    }
    $optimizer_translations['default'] =& $translator;
    return $translator;
}

function optimizer_apply_filters($hook, $value, ...$args) {
    return $value;
}

function optimizer_translate($value, $domain) {
    $translator = optimizer_nested_translator();
    $translated = $translator->translate($value);
    $translated = optimizer_apply_filters('gettext', $translated, $value, $domain);
    $translated = optimizer_apply_filters("gettext_{$domain}", $translated, $value, $domain);
    return $translated;
}

function optimizer_wrapper($value, $domain = 'default') {
    return optimizer_translate($value, $domain);
}

echo 'A';
optimizer_wrapper('zero');
echo 'B';
optimizer_wrapper('one');
echo 'C';
optimizer_wrapper('two');
echo 'D';
optimizer_wrapper('three');
echo 'E';
optimizer_wrapper('four');
echo 'F';
optimizer_wrapper('five');
echo 'G';
optimizer_wrapper('six');
echo 'H';
echo '[', optimizer_wrapper('Sunday'), ']', "\n";
