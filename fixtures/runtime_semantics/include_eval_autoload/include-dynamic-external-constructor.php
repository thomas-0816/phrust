<?php
spl_autoload_register(function ($class) {
    $files = array(
        'DynamicConstructorParent' => __DIR__ . '/_data/dynamic-constructor-parent.php',
        'DynamicConstructorChild' => __DIR__ . '/_data/dynamic-constructor-child.php',
    );
    if (isset($files[$class])) {
        require $files[$class];
    }
});

$instance = DynamicConstructorParent::make(DynamicConstructorChild::class, 'dynamic');
echo get_class($instance), ':', $instance->value(), "\n";
