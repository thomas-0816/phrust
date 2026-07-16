<?php
// runtime-semantics: category=include_eval_autoload expect=pass

include __DIR__ . '/_data/external-iterator-aggregate-child.php';

foreach (external_iterator_jar() as $cookie) {
    echo $cookie->name, "\n";
}

foreach (iterator_to_array(external_iterator_jar()) as $cookie) {
    echo "array:", $cookie->name, "\n";
}

foreach (external_direct_iterator() as $cookie) {
    echo "direct:", $cookie->name, "\n";
}
