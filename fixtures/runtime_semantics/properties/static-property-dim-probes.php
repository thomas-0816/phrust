<?php
// runtime-semantics: category=properties expect=pass
// Static property dimension probes stay inside the native static-state path.
class StaticDimProbe {
    private static array $values = [
        'present' => ['truthy' => 1, 'falsey' => 0],
        'null' => null,
    ];

    public static function run(): void {
        var_dump(isset(self::$values['present']['truthy']));
        var_dump(isset(self::$values['present']['missing']));
        var_dump(isset(self::$values['null']));
        var_dump(empty(self::$values['present']['truthy']));
        var_dump(empty(self::$values['present']['falsey']));
        var_dump(empty(self::$values['missing']['nested']));
        unset(self::$values['present']['truthy']);
        var_dump(isset(self::$values['present']['truthy']));
        var_dump(isset(self::$values['present']['falsey']));
    }
}

StaticDimProbe::run();
