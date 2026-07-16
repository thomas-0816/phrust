<?php
spl_autoload_register(function ($class) {
    $files = array(
        'ExternalMagicStaticParent' => __DIR__ . '/_data/external-magic-static-parent.php',
        'ExternalMagicStaticChild' => __DIR__ . '/_data/external-magic-static-child.php',
    );
    if (isset($files[$class])) {
        require $files[$class];
    }
});

echo ExternalMagicStaticChild::camelCaseContent('left', 'right'), "\n";
$instance = new ExternalMagicStaticChild();
echo $instance->camelCaseContent('left', 'right'), "\n";
