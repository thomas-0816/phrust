<?php
// runtime-semantics: category=include_eval_autoload expect=pass

function registered_parent_drop(string $class): void
{
    echo "drop:", $class, "\n";
}

function registered_parent_keep(string $class): void
{
    echo "keep:", $class, "\n";
}

spl_autoload_register('registered_parent_drop');
spl_autoload_register('registered_parent_keep');

include __DIR__ . '/_data/registered-callback-native-child.php';

var_dump(count(spl_autoload_functions()));
var_dump(class_exists('RegisteredCallbackMissing'));

$previous = set_error_handler(static function (int $level, string $message): bool {
    echo "error:", $level, ':', $message, "\n";
    return true;
});
var_dump($previous);
trigger_error('native callback state', E_USER_WARNING);
var_dump(restore_error_handler());

$exceptionHandler = static function (Throwable $throwable): void {
    echo "exception:", $throwable->getMessage(), "\n";
};
var_dump(set_exception_handler($exceptionHandler));
var_dump(restore_exception_handler());

echo "body\n";
