<?php

class StaticPropertyReferenceCollection {
    private static array $collections = array(
        'core' => array('count' => 1),
        'nested' => array('leaf' => array('count' => 10)),
    );

    public static function bump(string $key): int {
        $collection =& self::$collections[$key];
        return ++$collection['count'];
    }

    public static function bump_nested(string $first, string $second): int {
        $collection =& self::$collections[$first][$second];
        $collection['count'] += 5;
        return $collection['count'];
    }
}

var_dump(StaticPropertyReferenceCollection::bump('core'));
var_dump(StaticPropertyReferenceCollection::bump('core'));
var_dump(StaticPropertyReferenceCollection::bump_nested('nested', 'leaf'));
var_dump(StaticPropertyReferenceCollection::bump_nested('nested', 'leaf'));
