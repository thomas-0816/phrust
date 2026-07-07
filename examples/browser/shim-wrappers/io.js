import { error, streams, Pollable, poll as shimPoll } from '../node_modules/@bytecodealliance/preview2-shim/dist/browser/io.js';

// jco expects poll as { Pollable, poll } namespace object.
// Browser shim exports poll as a function with .Pollable property.
// Bridge the gap: re-export as { Pollable, poll: callableFunction }.

// Also fix: jco creates scratch pollable objects via
// Object.create(Pollable.prototype) in its trampoline code.
// Browser Pollable uses #private fields that throw on such objects.
// Patch prototype methods to catch and return safe defaults.

const _origReady = Pollable.prototype.ready;
const _origBlock = Pollable.prototype.block;
const _origSubscribe = Pollable.prototype.subscribe;

Pollable.prototype.ready = function () {
    try { return _origReady.call(this); }
    catch { return true; }
};
Pollable.prototype.block = function () {
    try { return _origBlock.call(this); }
    catch { return Promise.resolve(); }
};
Pollable.prototype.subscribe = function () {
    try { return _origSubscribe.call(this); }
    catch { return undefined; }
};

export const poll = { Pollable, poll: shimPoll };
export { error, streams };
