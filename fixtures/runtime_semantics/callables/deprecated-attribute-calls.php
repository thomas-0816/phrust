<?php
#[\Deprecated]
function old1() { return 1; }
#[\Deprecated(message: "use new2() instead")]
function old2() { return 2; }
var_dump(old1());
var_dump(old2());

// Methods and static methods carry the same call-site deprecation.
class C {
    #[\Deprecated]
    public function m() { return 1; }
    #[\Deprecated(message: "gone")]
    public static function s() { return 2; }
}
$c = new C();
var_dump($c->m());
var_dump(C::s());
