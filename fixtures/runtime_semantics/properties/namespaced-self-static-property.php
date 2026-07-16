<?php
namespace NativeComposerFixture;

class Loader {
    private static $includeFile;

    public static function initialize() {
        if (self::$includeFile !== null) {
            return;
        }
        self::$includeFile = static function ($value) {
            return "loaded:$value";
        };
    }

    public function load($value) {
        $includeFile = self::$includeFile;
        return $includeFile($value);
    }
}

Loader::initialize();
echo (new Loader())->load('ok'), "\n";
