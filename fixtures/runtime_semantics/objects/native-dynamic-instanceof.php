<?php

interface NativeDynamicMarker {}

class NativeDynamicRoot {}

class NativeDynamicChild extends NativeDynamicRoot implements NativeDynamicMarker {}

echo (new NativeDynamicChild()) instanceof (NativeDynamicRoot::class)
    ? "root:yes\n"
    : "root:no\n";

echo (new NativeDynamicChild()) instanceof (NativeDynamicMarker::class)
    ? "marker:yes\n"
    : "marker:no\n";

echo (new NativeDynamicChild()) instanceof (stdClass::class)
    ? "stdclass:yes\n"
    : "stdclass:no\n";
