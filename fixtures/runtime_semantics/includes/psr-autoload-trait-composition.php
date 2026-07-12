<?php
// runtime-semantics: category=includes expect=pass php_ref_required=1
// A PSR-4 autoloaded class composes a trait declared in a sibling file that
// is NOT loaded before the class file: reference PHP autoloads the trait at
// class-link time. The include compiler resolves the dependency through the
// explicit Composer PSR-4 metadata beside this fixture and still activates it
// through the registered callback.

spl_autoload_register(static function ($class) {
    echo 'autoload:', $class, "\n";
    $prefix = 'Acme\\';
    if (0 !== strncmp($class, $prefix, 5)) {
        return;
    }
    $file = __DIR__ . '/_data/psr-acme/src/Acme/'
        . str_replace('\\', '/', substr($class, 5)) . '.php';
    if (file_exists($file)) {
        require $file;
    }
});

$registry = new \Acme\Providers\Registry();
echo $registry->label(), "\n";
echo trait_exists(\Acme\Providers\Http\Traits\WithTransporterTrait::class) ? "trait-visible" : "trait-missing", "\n";
