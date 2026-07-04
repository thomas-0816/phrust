<?php
// runtime-semantics: category=properties expect=pass php_ref_required=1
// Property IC slot paths: hot declared reads/writes, visibility scopes,
// typed valid/invalid, readonly errors, dynamic/magic fallbacks, and a
// slot that becomes reference-backed after the cache is installed.
class Row {
    public $v = 0;
    private $secret = "s0";
    protected $shade = "p0";
    public int $typed = 1;
    public readonly int $ro;

    public function __construct() {
        $this->ro = 5;
    }

    public function bumpSecret() {
        $this->secret = $this->secret . "+";
        return $this->secret;
    }

    public function readShade() {
        return $this->shade;
    }
}

function writeV($o, $x) {
    $o->v = $x;
}

function readV($o) {
    return $o->v;
}

$r = new Row();
$sum = 0;
for ($i = 1; $i <= 8; $i++) {
    writeV($r, $i * 2);
    $sum += readV($r);
}
echo $sum, "|", $r->v, "\n";
echo $r->bumpSecret(), $r->bumpSecret(), "|", $r->readShade(), "\n";

$r->typed = 41;
echo $r->typed + 1, "\n";
try {
    $r->typed = "not an int";
} catch (TypeError $e) {
    echo "typed-rejected\n";
}
try {
    $r->ro = 6;
} catch (Error $e) {
    echo "readonly-rejected\n";
}

// Slot becomes reference-backed after the assign IC is hot: writes must
// go through the cell so the alias observes them.
$alias =& $r->v;
writeV($r, 99);
echo $alias, "|", $r->v, "\n";
$alias = 123;
echo readV($r), "\n";

// (Named class: anonymous-class property defaults are a separate known
// engine gap — they currently initialize to null.)
class MagicBag {
    public $real = "declared";
    public function __get($name) {
        return "magic:$name";
    }
}
$magic = new MagicBag();
echo $magic->real, "|", $magic->virtual, "\n";
