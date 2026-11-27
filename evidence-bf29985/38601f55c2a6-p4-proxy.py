"""TCP proxy in front of a yardd. Connection 1 (the hub's own) always passes.
Later ones follow the mode file: pass, blackhole (accept, never answer), or
stall (let the WebSocket handshake through, then stop reading the client and
reset it 0.5 s later)."""
import socket, struct, sys, threading, time

listen_port, target_port, mode_file = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
held = []
count = 0

def pump(src, dst, stop=None, spoke=None):
    try:
        while True:
            if stop is not None and stop.is_set():
                return
            data = src.recv(65536)
            if not data:
                break
            if spoke is not None:
                spoke.set()
            dst.sendall(data)
    except OSError:
        pass
    try:
        dst.shutdown(socket.SHUT_WR)
    except OSError:
        pass

def handle(client, n):
    mode = "pass" if n == 1 else open(mode_file).read().strip()
    print(f"conn {n}: {mode}", file=sys.stderr, flush=True)
    if mode == "blackhole":
        held.append(client)
        return
    target = socket.create_connection(("127.0.0.1", target_port))
    if mode == "pass":
        threading.Thread(target=pump, args=(client, target), daemon=True).start()
        threading.Thread(target=pump, args=(target, client), daemon=True).start()
        return
    spoke = threading.Event()
    threading.Thread(target=pump, args=(target, client, None, spoke), daemon=True).start()
    threading.Thread(target=pump, args=(client, target, spoke), daemon=True).start()
    spoke.wait()
    time.sleep(0.5)
    client.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, struct.pack("ii", 1, 0))
    # Wake the reader blocked on it first: a close while another thread is
    # in recv on the socket sends nothing until that recv returns.
    try:
        client.shutdown(socket.SHUT_RD)
    except OSError:
        pass
    time.sleep(0.05)
    client.close()
    target.close()
    print(f"conn {n}: reset", file=sys.stderr, flush=True)

srv = socket.socket()
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", listen_port))
srv.listen(16)
while True:
    c, _ = srv.accept()
    count += 1
    threading.Thread(target=handle, args=(c, count), daemon=True).start()
