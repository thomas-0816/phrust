<?php
class ReturnedBoundClosureScope {}

class ReturnedBoundClosureFactory {
    public static function makeRaw($value) {
        return function () use ($value) {
            return $value;
        };
    }

    public static function makeBound($value) {
        return Closure::bind(function () use ($value) {
            return $value;
        }, null, ReturnedBoundClosureScope::class);
    }
}
