<?php
// runtime-semantics: category=objects expect=pass
// Exact get_class_methods()/get_class_vars() over immutable class metadata.

class NativeMetadataBase
{
    public $publicBase = 1;
    protected $protectedBase = 2;
    private $privateBase = 3;
    public static $staticBase = 4;
    public int $typedBase;

    public function PublicBaseMethod() {}
    protected function ProtectedBaseMethod() {}
    private function PrivateBaseMethod() {}
}

class NativeMetadataChild extends NativeMetadataBase
{
    public $publicChild = 5;
    protected $protectedChild = 6;
    private $privateChild = 7;
    public static $staticChild = 8;
    public array $directArray = ["native" => 9];
    public string $typedChild;

    public function PublicChildMethod() {}
    protected function ProtectedChildMethod() {}
    private function PrivateChildMethod() {}

    public static function inspectInside(): array
    {
        return [
            get_class_methods(self::class),
            get_class_vars(self::class),
        ];
    }
}

function nativeClassMetadataFamily(): array
{
    return [
        get_class_methods(NativeMetadataChild::class),
        get_class_methods(new NativeMetadataChild()),
        get_class_vars(NativeMetadataChild::class),
        NativeMetadataChild::inspectInside(),
    ];
}

var_dump(nativeClassMetadataFamily());
