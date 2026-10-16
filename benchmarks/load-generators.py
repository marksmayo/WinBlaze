"""Background load for conditioned extended medal events (run until killed).

python benchmarks/load-generators.py cpu            busy loop on one core (start one per core)
python benchmarks/load-generators.py disk DIR       sequential 256 MiB writes with flush, rewritten forever
python benchmarks/load-generators.py churn DIR      create and delete small files in DIR continuously
"""
import os
import sys
import time


def cpu():
    x = 0
    while True:
        x = (x * 1103515245 + 12345) & 0xFFFFFFFF


def disk(folder):
    os.makedirs(folder, exist_ok=True)
    block = os.urandom(1024 * 1024)
    path = os.path.join(folder, 'busy.bin')
    while True:
        with open(path, 'wb') as handle:
            for _ in range(256):
                handle.write(block)
            handle.flush()
            os.fsync(handle.fileno())


def churn(folder):
    os.makedirs(folder, exist_ok=True)
    i = 0
    while True:
        path = os.path.join(folder, f'c{i % 2000:04}.tmp')
        if os.path.exists(path):
            os.remove(path)
        else:
            with open(path, 'wb') as handle:
                handle.write(b'x' * 512)
        i += 1
        if i % 200 == 0:
            time.sleep(0.001)


if __name__ == '__main__':
    mode = sys.argv[1]
    if mode == 'cpu':
        cpu()
    elif mode == 'disk':
        disk(sys.argv[2])
    elif mode == 'churn':
        churn(sys.argv[2])
    else:
        sys.exit(f'unknown mode {mode}')
