<?php
// runtime-semantics: category=objects expect=pass

$directory = __DIR__ . '/_data/recursive-directory-iterator';
$iterator = new RegexIterator(
    new RecursiveIteratorIterator(new RecursiveDirectoryIterator($directory)),
    '/^.+\.html$/i',
    RecursiveRegexIterator::GET_MATCH
);

$files = [];
foreach (iterator_to_array($iterator) as $path => $matches) {
    $files[] = basename($path) . ':' . (is_array($matches) ? 'matches' : 'value');
}
sort($files);
var_dump($files);
