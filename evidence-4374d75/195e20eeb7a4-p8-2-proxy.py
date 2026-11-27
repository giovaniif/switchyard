"""TCP proxy hub -> box that loses the answer to chosen ops.
args: listen-port target-port arm-file log-file
The arm file holds op names, comma separated (say "mcp.add,mcp.remove").
Every WebSocket frame the hub sends is forwarded whole. When one carries an
armed op, that op is taken off the arm file (one shot), everything the box
sends on that connection from then on is dropped (its answer), and both sides
are closed 1 s later."""
import socket, sys, threading, time
listen, target, armfile, logfile = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3], sys.argv[4]
lock = threading.Lock()
def log(msg):
    with open(logfile, "a") as f:
        f.write(f"{time.time():.3f} {msg}\n")
def armed_hit(payload):
    with lock:
        ops = [o for o in open(armfile).read().strip().split(",") if o]
        for o in ops:
            if (b'"op":"' + o.encode() + b'"') in payload:
                ops.remove(o)
                open(armfile, "w").write(",".join(ops))
                return o
    return None
def frames(buf):
    """Complete client frames at the start of buf: (payloads, bytes used)."""
    out, i = [], 0
    while True:
        if len(buf) - i < 2:
            break
        n = buf[i + 1] & 0x7F
        masked = buf[i + 1] & 0x80
        j = i + 2
        if n == 126:
            if len(buf) - j < 2: break
            n = int.from_bytes(buf[j:j + 2], "big"); j += 2
        elif n == 127:
            if len(buf) - j < 8: break
            n = int.from_bytes(buf[j:j + 8], "big"); j += 8
        key = b""
        if masked:
            if len(buf) - j < 4: break
            key = buf[j:j + 4]; j += 4
        if len(buf) - j < n:
            break
        data = buf[j:j + n]
        if masked and n <= 65536:
            data = bytes(b ^ key[k % 4] for k, b in enumerate(data))
        out.append(data if n <= 65536 else b"")
        i = j + n
    return out, i
def handle(c, n):
    s = socket.create_connection(("127.0.0.1", target))
    st = {"hs": False, "swallowing": False, "buf": b""}
    log(f"conn {n} open")
    def kill():
        log(f"conn {n} closed by proxy")
        for x in (c, s):
            try: x.shutdown(socket.SHUT_RDWR)
            except OSError: pass
            x.close()
    def up():
        while True:
            try: d = c.recv(65536)
            except OSError: break
            if not d: break
            try: s.sendall(d)
            except OSError: break
            if not st["hs"]:
                st["buf"] += d
                k = st["buf"].find(b"\r\n\r\n")
                if k < 0: continue
                st["hs"] = True
                st["buf"] = st["buf"][k + 4:]
            else:
                st["buf"] += d
            got, used = frames(st["buf"])
            st["buf"] = st["buf"][used:]
            for payload in got:
                if st["swallowing"]: continue
                op = armed_hit(payload)
                if op:
                    st["swallowing"] = True
                    log(f"conn {n} forwarded {op} whole ({len(payload)} bytes); dropping the box's answer, closing in 1 s")
                    threading.Timer(1.0, kill).start()
    def down():
        while True:
            try: d = s.recv(65536)
            except OSError: break
            if not d: break
            if st["swallowing"]:
                log(f"conn {n} dropped {len(d)} bytes from the box")
                continue
            try: c.sendall(d)
            except OSError: break
    threading.Thread(target=up, daemon=True).start()
    threading.Thread(target=down, daemon=True).start()
srv = socket.socket(); srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", listen)); srv.listen(16)
n = 0
while True:
    c, _ = srv.accept(); n += 1
    handle(c, n)
