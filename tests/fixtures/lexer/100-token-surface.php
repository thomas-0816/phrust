<?php
namespace Surface\Tokens;

use Foo\Bar as Baz;

abstract final readonly class Example extends Base implements Iface
{
    use TraitA, TraitB {
        TraitA::method insteadof TraitB;
        TraitB::method as otherMethod;
    }

    public(set) protected(set) private(set)
    public const VALUE = 1;
    private static $prop;
    protected $other;
    var $legacy;

    public function &method(?int $x): void
    {
        declare(ticks=1): enddeclare;
        global $g;
        static $s;
        callable $cb;
        echo "value $x";
        print $x;
        clone new self();
        if ($x): elseif ($x === 2): else: endif;
        while ($x): break; endwhile;
        do { continue; } while ($x);
        for ($i = 0; $i < 3; $i++): endfor;
        foreach ([1, 2] as $k => $v): endforeach;
        switch ($x): case 1: break; default: endswitch;
        try { throw new \Exception(); } catch (\Throwable $e) { } finally { }
        include "a.php";
        include_once "b.php";
        require "c.php";
        require_once "d.php";
        eval("return 1;");
        isset($x);
        empty($x);
        unset($x);
        list($a) = array(1);
        (array) $x;
        (double) $x;
        (object) $x;
        (string) $x;
        (unset) $x;
        $x || $g;
        $x == $g;
        $x >= $g;
        $x != $g;
        $x <= $g;
        $x .= "a";
        $x /= 2;
        $x -= 1;
        $x %= 2;
        $x *= 3;
        $x |= 1;
        $x ** 2;
        $x << 1;
        $x <<= 1;
        $x >> 1;
        $x >>= 1;
        $x ^= 1;
        $x instanceof Baz;
        fn($y) => $y;
        match ($x) { default => null };
        yield $x;
        yield from $x;
        goto done;
        done:
        exit;
    }
}

interface Iface {}
trait TraitA { public function method() {} }
trait TraitB { public function method() {} }
enum Suit { case Hearts; }

__halt_compiler();
