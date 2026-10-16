"""Deterministic fixtures for the extended medal events.

Usage: python benchmarks/make-extended-datasets.py [--root DIR] [--clean] NAME [NAME ...]
Each dataset is written to <root>/<name> with a sibling <name>.manifest.json
holding the expected file/directory/logical-byte totals (excluding the root).
"""
import argparse
import json
import math
import os
import random
import shutil
import subprocess
import sys

LONG = '\\\\?\\'


def long_path(path):
    path = os.path.abspath(path)
    return path if path.startswith(LONG) else LONG + path


def set_sparse_length(path, length):
    """Mark a file sparse and extend it without writing zeros.

    Python's truncate() goes through the C runtime's _chsize, which zero-fills
    the extension and allocates every byte; SetEndOfFile on a sparse file does not.
    """
    import ctypes
    from ctypes import wintypes
    subprocess.run(['fsutil', 'sparse', 'setflag', path], check=True, capture_output=True)
    kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel32.CreateFileW.restype = wintypes.HANDLE
    handle = kernel32.CreateFileW(long_path(path), 0x40000000, 0, None, 3, 0x80, None)
    if handle == wintypes.HANDLE(-1).value:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        if not kernel32.SetFilePointerEx(handle, ctypes.c_longlong(length), None, 0) or not kernel32.SetEndOfFile(handle):
            raise ctypes.WinError(ctypes.get_last_error())
    finally:
        kernel32.CloseHandle(handle)
    allocated = allocated_bytes(path)
    if allocated > 16 * 1024 * 1024:
        raise RuntimeError(f'{path} is not sparse: {allocated} bytes allocated')


def allocated_bytes(path):
    import ctypes
    high = ctypes.c_ulong(0)
    low = ctypes.windll.kernel32.GetCompressedFileSizeW(long_path(path), ctypes.byref(high))
    return (high.value << 32) + (low & 0xFFFFFFFF)


def write(path, size=0, data=None):
    with open(long_path(path), 'wb') as handle:
        if data is not None:
            handle.write(data)
        elif size:
            handle.write(b'\0' * size)


def million(root):
    """1,000,000 empty files in 1,000 folders."""
    for d in range(1000):
        folder = os.path.join(root, f'd{d:04}')
        os.mkdir(folder)
        for f in range(1000):
            open(os.path.join(folder, f'f{f:04}'), 'wb').close()
    return 1_000_000, 1000, 0


def deep(root):
    """Two chains 200 folders deep with 40-character names (paths over 8,000 chars)."""
    files = dirs = 0
    for chain in range(2):
        path = os.path.join(root, f'chain{chain}')
        os.mkdir(long_path(path)); dirs += 1
        for level in range(199):
            path = os.path.join(path, f'level{level:03}_' + 'x' * 31)
            os.mkdir(long_path(path)); dirs += 1
            write(os.path.join(path, 'file.txt'), data=b'deep'); files += 1
    return files, dirs, files * 4


def dirs500k(root):
    """500,000 empty folders under 500 parents."""
    for p in range(500):
        parent = os.path.join(root, f'p{p:03}')
        os.mkdir(parent)
        for c in range(999):
            os.mkdir(os.path.join(parent, f'c{c:03}'))
    return 0, 500 * 1000, 0


def sparse50g(root):
    """One 50 GiB sparse file."""
    path = os.path.join(root, 'huge.bin')
    open(path, 'wb').close()
    set_sparse_length(path, 50 * 1024 ** 3)
    return 1, 0, 50 * 1024 ** 3


def hardlinks(root):
    """20,000 files with four extra hard links each (100,000 names)."""
    os.mkdir(os.path.join(root, 'originals'))
    for g in range(4):
        os.mkdir(os.path.join(root, f'links{g}'))
    for i in range(20000):
        original = os.path.join(root, 'originals', f'f{i:05}.dat')
        write(original, data=b'%d' % i * 64)
        for g in range(4):
            os.link(original, os.path.join(root, f'links{g}', f'f{i:05}.dat'))
    logical = sum(len(b'%d' % i * 64) for i in range(20000)) * 5
    return 100_000, 5, logical


def sparsecompressed(root):
    """1,000 sparse 64 MiB files and 1,000 NTFS-compressed 256 KiB files."""
    sparse_dir = os.path.join(root, 'sparse'); os.mkdir(sparse_dir)
    comp_dir = os.path.join(root, 'compressed'); os.mkdir(comp_dir)
    for i in range(1000):
        path = os.path.join(sparse_dir, f's{i:04}.bin')
        open(path, 'wb').close()
        set_sparse_length(path, 64 * 1024 ** 2)
        with open(path, 'r+b') as handle:  # One real 4-byte extent per file.
            handle.seek(i * 4096)
            handle.write(b'data')
    subprocess.run(['compact', '/c', '/q', comp_dir], check=True, capture_output=True)
    text = (b'compressible text ' * 16384)[:262144]
    for i in range(1000):
        write(os.path.join(comp_dir, f'c{i:04}.txt'), data=text)
    return 2000, 2, 1000 * 64 * 1024 ** 2 + 1000 * 262144


def ads(root):
    """5,000 files, each with two 1 KiB alternate data streams."""
    for i in range(5000):
        path = os.path.join(root, f'f{i:04}.txt')
        write(path, data=b'main')
        for stream in ('one', 'two'):
            with open(f'{path}:{stream}', 'wb') as handle:
                handle.write(b's' * 1024)
    return 5000, 0, 5000 * 4


def junctions(root):
    """A 3-level tree with junctions back to the root and across branches (loops)."""
    files = dirs = 0
    for a in range(10):
        for b in range(10):
            folder = os.path.join(root, f'a{a}', f'b{b}')
            os.makedirs(folder); dirs += 1 + (b == 0)
            for f in range(10):
                write(os.path.join(folder, f'f{f}.txt'), data=b'j'); files += 1
            # Loop back to the dataset root and a cross link to a sibling branch.
            for name, target in (('loop', root), ('cross', os.path.join(root, f'a{(a + 1) % 10}'))):
                link = os.path.join(folder, name)
                subprocess.run(['cmd', '/c', 'mklink', '/J', link, target], check=True, capture_output=True)
                dirs += 1
    return files, dirs, files


def unicode_names(root):
    """50,000 files with CJK, emoji, right-to-left and combining-mark names."""
    pieces = ['文件', 'ファイル', '파일', '📁', '🦀', 'ملف', 'קובץ', 'é', 'Ωμέγα', 'Ünïcödé']
    rng = random.Random(7)
    for d in range(50):
        folder = os.path.join(root, f'{pieces[d % len(pieces)]}_{d:02}')
        os.mkdir(folder)
        for f in range(1000):
            name = ''.join(rng.choice(pieces) for _ in range(3)) + f'_{f:03}.txt'
            write(os.path.join(folder, name), data=b'u')
    return 50_000, 50, 50_000


def denied(root):
    """A readable tree plus a 1,000-file subtree the current user cannot list."""
    open_dir = os.path.join(root, 'readable'); os.mkdir(open_dir)
    for i in range(1000):
        write(os.path.join(open_dir, f'r{i:04}.txt'), data=b'r')
    blocked = os.path.join(root, 'denied'); os.mkdir(blocked)
    for i in range(1000):
        write(os.path.join(blocked, f'd{i:04}.txt'), data=b'd')
    user = os.environ.get('USERNAME')
    subprocess.run(['icacls', blocked, '/deny', f'{user}:(OI)(CI)(RD,X)'], check=True, capture_output=True)
    # Visible totals: the readable files plus both folders.
    return 1000, 2, 1000


def nodemodules(root):
    """~200,000 small files in a deep, package-like layout."""
    rng = random.Random(11)
    files = dirs = logical = 0
    for p in range(400):
        package = os.path.join(root, 'node_modules', f'pkg{p:03}')
        for sub in ('lib', 'dist', os.path.join('lib', 'internal'), os.path.join('dist', 'esm', 'util')):
            os.makedirs(os.path.join(package, sub))
        dirs += 5 + 1 + 1 + 3
        for f in range(500):
            sub = ('lib', 'dist', os.path.join('lib', 'internal'), os.path.join('dist', 'esm', 'util'))[f % 4]
            size = rng.randint(50, 4000)
            write(os.path.join(package, sub, f'm{f:03}.js'), data=b'j' * size)
            files += 1; logical += size
    return files, dirs + 1, logical


def tengb(root):
    """10,240 files of 1 MiB (incompressible pattern), 10 GiB allocated."""
    block = random.Random(3).randbytes(1024 * 1024)
    for d in range(10):
        folder = os.path.join(root, f'd{d}')
        os.mkdir(folder)
        for f in range(1024):
            write(os.path.join(folder, f'f{f:04}.bin'), data=block)
    return 10240, 10, 10240 * 1024 * 1024


def lognormal(root):
    """250,000 files with lognormal sizes (median ~4 KiB, capped at 8 MiB)."""
    rng = random.Random(5)
    logical = 0
    for d in range(250):
        folder = os.path.join(root, f'd{d:03}')
        os.mkdir(folder)
        for f in range(1000):
            size = min(int(math.exp(rng.gauss(8.3, 1.6))), 8 * 1024 * 1024)
            write(os.path.join(folder, f'f{f:04}.dat'), size=size)
            logical += size
    return 250_000, 250, logical


def fragmented(root):
    """One folder that held 300,000 files; 290,000 deleted, leaving 10,000."""
    folder = os.path.join(root, 'churned'); os.mkdir(folder)
    for i in range(300_000):
        open(os.path.join(folder, f'f{i:06}'), 'wb').close()
    for i in range(300_000):
        if i % 30:
            os.remove(os.path.join(folder, f'f{i:06}'))
    return 10_000, 1, 0


FILLER_FILES = 247_450
FILLER_DIRS = 53_693


def filler(root):
    """Restores C:'s competitor-best population (3,334,037 files, 849,811 folders).

    An external cache cleanup on 2026-10-14 removed this many files and folders
    outside the datasets. Generate it under its own --root (artifacts/population-filler)
    so it stays present for both full-drive stages. Nested folders with fanout 38
    and a cache-like size mix (median ~1.5 KiB, capped at 4 MiB, ~1 GiB in total).
    """
    rng = random.Random(11)
    folders, frontier = [], [root]
    while len(folders) < FILLER_DIRS:
        parent = frontier.pop(0)
        for i in range(min(38, FILLER_DIRS - len(folders))):
            folder = os.path.join(parent, f'p{i:02}')
            os.mkdir(folder)
            folders.append(folder)
            frontier.append(folder)
    logical = 0
    for f in range(FILLER_FILES):
        size = min(int(math.exp(rng.gauss(7.3, 1.5))), 4 * 1024 * 1024)
        write(os.path.join(folders[f % FILLER_DIRS], f'f{f:06}.bin'), size=size)
        logical += size
    return FILLER_FILES, FILLER_DIRS, logical


DATASETS = {f.__name__.replace('_names', ''): f for f in (
    million, deep, dirs500k, sparse50g, hardlinks, sparsecompressed, ads, junctions,
    unicode_names, denied, nodemodules, tengb, lognormal, fragmented, filler)}


# Approximate allocated bytes each dataset needs; generation refuses to start
# unless this much plus MIN_FREE_AFTER stays free on the target volume.
SPACE_NEEDED = {'tengb': 11 * 1024 ** 3, 'lognormal': 5 * 1024 ** 3, 'million': 1024 ** 3,
                'dirs500k': 1024 ** 3, 'nodemodules': 2 * 1024 ** 3, 'sparsecompressed': 300 * 1024 ** 2,
                'filler': 2 * 1024 ** 3}
MIN_FREE_AFTER = 15 * 1024 ** 3


def ensure_space(name, root):
    free = shutil.disk_usage(root).free
    needed = SPACE_NEEDED.get(name, 200 * 1024 ** 2)
    if free - needed < MIN_FREE_AFTER:
        raise SystemExit(f'{name}: needs ~{needed >> 20} MiB but only {free >> 20} MiB is free; '
                         f'refusing to leave less than {MIN_FREE_AFTER >> 30} GiB')


def remove(path):
    if not os.path.exists(path):
        return
    blocked = os.path.join(path, 'denied')
    if os.path.exists(blocked):
        subprocess.run(['icacls', blocked, '/reset', '/T', '/Q'], capture_output=True)
    # Remove junctions first so rmtree never follows them.
    for dirpath, dirnames, _ in os.walk(long_path(path), topdown=True, followlinks=False):
        for name in list(dirnames):
            full = os.path.join(dirpath, name)
            if os.path.isjunction(full):
                os.rmdir(full)
                dirnames.remove(name)
    shutil.rmtree(long_path(path))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('names', nargs='+', choices=sorted(DATASETS))
    parser.add_argument('--root', default=os.path.join(os.path.dirname(__file__), '..', 'artifacts', 'extended-datasets'))
    parser.add_argument('--clean', action='store_true')
    args = parser.parse_args()
    os.makedirs(args.root, exist_ok=True)
    for name in args.names:
        target = os.path.abspath(os.path.join(args.root, name))
        manifest = target + '.manifest.json'
        if os.path.exists(manifest) and not args.clean:
            print(f'{name}: exists'); continue
        remove(target)
        ensure_space(name, args.root)
        os.mkdir(target)
        files, dirs, logical = DATASETS[name](target)
        with open(manifest, 'w', encoding='utf-8') as handle:
            json.dump({'name': name, 'root': target, 'files': files, 'directories': dirs, 'bytes': logical}, handle, indent=2)
        print(f'{name}: {files} files, {dirs} dirs, {logical} bytes')


if __name__ == '__main__':
    sys.exit(main())
