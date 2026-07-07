import { pollableCreate } from '../node_modules/@bytecodealliance/preview2-shim/dist/browser/io.js';

class Network {}
class ResolveAddressStream {
    resolveNextAddress() { return undefined; }
    subscribe() { return pollableCreate(); }
}
function resolveAddresses(network, name) { return new ResolveAddressStream(); }

class TcpSocket {
    startBind(network, address) {}
    finishBind() {}
    startConnect(network, address) {}
    finishConnect() {}
    startListen() {}
    finishListen() {}
    isListening() { return false; }
    accept() { throw { tag: 'closed' }; }
    stream(mode) { return [null, null]; }
    setListenBacklogSize(size) {}
    localAddress() { return null; }
    remoteAddress() { return null; }
    subscribe() { return pollableCreate(); }
    shutdown(shutdownType) {}
}
function createTcpSocket(addressFamily) { return new TcpSocket(); }

class IncomingDatagramStream {
    receive(maxResults) { return []; }
    subscribe() { return pollableCreate(); }
}
class OutgoingDatagramStream {
    checkSend() { return 0n; }
    send(datagrams) {}
}

class UdpSocket {
    startBind(network, address) {}
    finishBind() {}
    stream(mode) { return [new IncomingDatagramStream(), new OutgoingDatagramStream()]; }
    localAddress() { return null; }
    remoteAddress() { return null; }
    receive(maxResults) { return []; }
    checkSend() { return 0n; }
    send(datagrams) {}
    unicastHopLimit() { return 0; }
    setUnicastHopLimit(limit) {}
    receiveBufferSize() { return 0n; }
    setReceiveBufferSize(size) {}
    sendBufferSize() { return 0n; }
    setSendBufferSize(size) {}
    subscribe() { return pollableCreate(); }
}
function createUdpSocket(addressFamily) { return new UdpSocket(); }

export const instanceNetwork = {
    instanceNetwork() { return new Network(); },
};
export const ipNameLookup = {
    ResolveAddressStream,
    resolveAddresses,
};
export const network = {
    Network,
};
export const tcp = {
    TcpSocket,
};
export const tcpCreateSocket = {
    createTcpSocket,
};
export const udp = {
    IncomingDatagramStream,
    OutgoingDatagramStream,
    UdpSocket,
};
export const udpCreateSocket = {
    createUdpSocket,
};
