<?php

interface Marker {}

enum Status: string implements Marker {
    case Draft = "draft";
    case Published = "published";
}

enum PureStatus {
    case Open;
    case Closed;
}
