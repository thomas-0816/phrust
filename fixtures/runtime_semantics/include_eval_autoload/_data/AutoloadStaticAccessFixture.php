<?php
class AutoloadStaticAccessFixture {
    public const VALUE = "const";
    public static string $prop = "prop";

    public static function method(): string {
        return "method";
    }
}
