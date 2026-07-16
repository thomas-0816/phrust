<?php
// runtime-semantics: expect=pass
// A large unit forces the bounded local call graph to use native trampolines.
function native_depth_00(): string { return native_depth_01(); }
function native_depth_01(): string { return native_depth_02(); }
function native_depth_02(): string { return native_depth_03(); }
function native_depth_03(): string { return native_depth_04(); }
function native_depth_04(): string { return native_depth_05(); }
function native_depth_05(): string { return native_depth_06(); }
function native_depth_06(): string { return native_depth_07(); }
function native_depth_07(): string { return native_depth_08(); }
function native_depth_08(): string { return native_depth_09(); }
function native_depth_09(): string { return native_depth_10(); }
function native_depth_10(): string { return native_depth_11(); }
function native_depth_11(): string { return native_depth_12(); }
function native_depth_12(): string { return native_depth_13(); }
function native_depth_13(): string { return native_depth_14(); }
function native_depth_14(): string { return native_depth_15(); }
function native_depth_15(): string { return native_depth_16(); }
function native_depth_16(): string { return native_depth_17(); }
function native_depth_17(): string { return native_depth_18(); }
function native_depth_18(): string { return native_depth_19(); }
function native_depth_19(): string { return native_depth_20(); }
function native_depth_20(): string { return native_depth_21(); }
function native_depth_21(): string { return native_depth_22(); }
function native_depth_22(): string { return native_depth_23(); }
function native_depth_23(): string { return native_depth_24(); }
function native_depth_24(): string { return native_depth_25(); }
function native_depth_25(): string { return native_depth_26(); }
function native_depth_26(): string { return native_depth_27(); }
function native_depth_27(): string { return native_depth_28(); }
function native_depth_28(): string { return native_depth_29(); }
function native_depth_29(): string { return native_depth_30(); }
function native_depth_30(): string { return native_depth_31(); }
function native_depth_31(): string { return native_depth_32(); }
function native_depth_32(): string { return native_depth_33(); }
function native_depth_33(): string { return native_depth_34(); }
function native_depth_34(): string { return native_depth_35(); }
function native_depth_35(): string { return native_depth_36(); }
function native_depth_36(): string { return native_depth_37(); }
function native_depth_37(): string { return native_depth_38(); }
function native_depth_38(): string { return native_depth_39(); }
function native_depth_39(): string { return native_depth_40(); }
function native_depth_40(): string { return native_depth_41(); }
function native_depth_41(): string { return native_depth_42(); }
function native_depth_42(): string { return native_depth_43(); }
function native_depth_43(): string { return native_depth_44(); }
function native_depth_44(): string { return native_depth_45(); }
function native_depth_45(): string { return native_depth_46(); }
function native_depth_46(): string { return native_depth_47(); }
function native_depth_47(): string { return 'depth-ok'; }

echo native_depth_00(), "\n";
