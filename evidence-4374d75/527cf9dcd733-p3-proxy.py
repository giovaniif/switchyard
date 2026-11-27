"""TCP proxy hub -> box. Each new connection reads the mode file once:
pass   forward both ways.
swallow  forward both ways until the client's first frame after the
         WebSocket handshake (the page's op) has gone to the box; from then
         on drop what the box sends (its answer) and close both sides 1 s later.
         One-shot: the mode file goes back to pass."""
import socket, sys, threading, time
listen, target, modefile, logfile = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3], sys.argv[4]
def log(msg):
    with open(logfile, "a") as f:
        f.write(f"{time.time():.3f} {msg}\n")
def handle(c, n):
    mode = open(modefile).read().strip()
    if mode == "swallow":
        open(modefile, "w").write("pass")
    s = socket.create_connection(("127.0.0.1", target))
    st = {"hs": False, "swallowing": False}
    log(f"conn {n} mode={mode}")
    def kill():
        log(f"conn {n} closed by proxy")
        for x in (c, s):
            try:
                x.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            x.close()
    def up():
        while True:
            try:
                d = c.recv(65536)
            except OSError:
                break
            if not d:
                break
            try:
                s.sendall(d)
            except OSError:
                break
            if not st["hs"]:
                st["hs"] = b"\r\n\r\n" in d
            elif mode == "swallow" and not st["swallowing"]:
                st["swallowing"] = True
                log(f"conn {n} op forwarded ({len(d)} bytes); swallowing the box's answer")
                threading.Timer(1.0, kill).start()
    def down():
        while True:
            try:
                d = s.recv(65536)
            except OSError:
                break
            if not d:
                break
            if st["swallowing"]:
                log(f"conn {n} swallowed {len(d)} bytes from the box")
                continue
            try:
                c.sendall(d)
            except OSError:
                break
    threading.Thread(target=up, daemon=True).start()
    threading.Thread(target=down, daemon=True).start()
srv = socket.socket(); srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", listen)); srv.listen(16)
n = 0
while True:
    c, _ = srv.accept(); n += 1
    handle(c, n)
