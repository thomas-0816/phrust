<?php
interface X {} interface Y {}
class Impl implements X, Y {}
class NotBoth implements X {}
class Holder {
    public X&Y $prop;
    public int $count;
}
$h = new Holder();
$h->prop = new Impl();
try { $h->prop = new NotBoth(); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $h->prop = "text"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $h->count = []; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo "done\n";
