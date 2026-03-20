import argparse
import socket
import sys
import threading

def recv_loop(sock: socket.socket) -> None:
    buf = b""
    try:
        while True:
            data = sock.recv(4096)
            if not data:
                print("\n[peer disconnected]")
                break
            buf += data
            while b"\n" in buf:
                line, buf = buf.split(b"\n", 1)
                text = line.decode("utf-8", errors="replace")
                print(f"\rpeer: {text}\n> ", end="", flush=True)
    except Exception as e:
        print(f"\n[recv error: {e}]")

def send_loop(sock: socket.socket) -> None:
    try:
        while True:
            line = input("> ")
            sock.sendall((line + "\n").encode("utf-8"))
    except EOFError:
        pass
    except KeyboardInterrupt:
        pass
    finally:
        try:
            sock.shutdown(socket.SHUT_RDWR)
        except Exception:
            pass
        sock.close()

def listen(port: int) -> None:
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("0.0.0.0", port))
    srv.listen(1)
    print(f"[listening on 0.0.0.0:{port}]")
    conn, addr = srv.accept()
    print(f"[connected from {addr[0]}:{addr[1]}]")
    threading.Thread(target=recv_loop, args=(conn,), daemon=True).start()
    send_loop(conn)

def connect(host: str, port: int) -> None:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect((host, port))
    print(f"[connected to {host}:{port}]")
    threading.Thread(target=recv_loop, args=(sock,), daemon=True).start()
    send_loop(sock)

def main() -> None:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="mode", required=True)

    lp = sub.add_parser("listen")
    lp.add_argument("--port", type=int, default=9000)

    cp = sub.add_parser("connect")
    cp.add_argument("host")
    cp.add_argument("--port", type=int, default=9000)

    args = ap.parse_args()

    if args.mode == "listen":
        listen(args.port)
    else:
        connect(args.host, args.port)

if __name__ == "__main__":
    main()
