import ctypes
import os
import platform


class PublicationConflict(RuntimeError):
    pass


def fingerprint(path):
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        return None
    identity = (metadata.st_dev, metadata.st_ino, metadata.st_mode,
                metadata.st_size, metadata.st_mtime_ns)
    if path.is_symlink():
        return identity, os.readlink(path)
    if path.is_dir():
        return identity, tuple((child.name, fingerprint(child)) for child in sorted(path.iterdir()))
    return identity


def atomic_rename(source, destination, *, exchange):
    libc = ctypes.CDLL(None, use_errno=True)
    paths = (os.fsencode(source), os.fsencode(destination))
    if platform.system() == "Darwin":
        operation = libc.renamex_np(*paths, 0x02 if exchange else 0x04)
    else:
        operation = libc.renameat2(-100, paths[0], -100, paths[1], 0x02 if exchange else 0x01)
    if operation:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), source, None, destination)


class Publication:
    def __init__(self, live, staged, prior, label):
        self.live = live
        self.staged = staged
        self.prior = prior
        self.label = label
        self.installed = fingerprint(staged)
        if self.installed is None:
            raise ValueError(f"缺少待发布的{label}：{staged}")
        self.applied = False

    def conflict(self):
        return PublicationConflict(f"{self.label}冲突：检测到其他进程的修改，请核对当前状态后重试")

    def publish(self):
        if fingerprint(self.live) != self.prior:
            raise self.conflict()
        self.live.parent.mkdir(parents=True, exist_ok=True)
        try:
            atomic_rename(self.staged, self.live, exchange=self.prior is not None)
        except (FileExistsError, FileNotFoundError) as error:
            raise self.conflict() from error
        self.applied = True
        if fingerprint(self.staged) != self.prior:
            raise self.conflict()
        self.verify()

    def verify(self):
        if fingerprint(self.live) != self.installed:
            raise self.conflict()

    def restore(self):
        if not self.applied:
            return
        if fingerprint(self.live) != self.installed:
            raise self.conflict()
        saved = fingerprint(self.staged)
        if saved is None:
            atomic_rename(self.live, self.staged, exchange=False)
            if fingerprint(self.staged) != self.installed:
                atomic_rename(self.staged, self.live, exchange=False)
                raise self.conflict()
        else:
            atomic_rename(self.live, self.staged, exchange=True)
            if fingerprint(self.staged) != self.installed:
                if fingerprint(self.live) == saved:
                    atomic_rename(self.live, self.staged, exchange=True)
                raise self.conflict()
