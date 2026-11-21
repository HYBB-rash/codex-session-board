import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from update_plugin import command_error, update, update_lock, write_record


REPOSITORY = "https://github.com/HYBB-rash/codex-session-board.git"
STATUS = "board-update.json"


def start(state, codex, running_build):
    try:
        with update_lock(state) as lock:
            launch(state, codex, running_build, lock.fileno())
    except BlockingIOError:
        if not (state / STATUS).exists():
            raise RuntimeError("Another plugin installation is in progress") from None


def launch(state, codex, running_build, descriptor):
    snapshot = Path(tempfile.mkdtemp(prefix="updater-", dir=state))
    try:
        shutil.copytree(Path(__file__).resolve().parent, snapshot, dirs_exist_ok=True)
        write_record(state / STATUS, {"phase": "fetching"})
        arguments = [sys.executable, "-EBS", str(snapshot / Path(__file__).name),
                     str(state), codex, running_build, "--worker", str(descriptor)]
        with (state / "board-update.log").open("w") as log:
            subprocess.Popen(arguments, cwd=state, stdin=subprocess.DEVNULL,
                             stdout=log, stderr=log, start_new_session=True, pass_fds=(descriptor,))
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        write_record(state / STATUS, {"phase": "failed", "failedStage": "fetching", "error": str(error)})
        shutil.rmtree(snapshot)
        raise


def work(state, codex, running_build, descriptor):
    snapshot = Path(__file__).resolve().parent
    if snapshot.parent != state or not snapshot.name.startswith("updater-"):
        raise ValueError("Update worker must run from its private snapshot")
    if not os.path.samestat(os.fstat(descriptor), (state / "update.lock").stat()):
        raise ValueError("Update worker must inherit the installation lock")
    stage = {"phase": "fetching"}

    def progress(phase, **fields):
        stage.clear()
        stage.update(phase=phase, **fields)
        write_record(state / STATUS, stage)

    try:
        result = update(REPOSITORY, "candidate", None, state, codex,
                        inherited_lock=descriptor, progress=progress, running_build=running_build)
        progress(result["phase"], version=result["release"]["version"],
                 build=result["release"]["binary_sha256"], installed=True,
                 error=result.get("reload_error"))
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        progress("failed", failedStage=stage["phase"], version=stage.get("version"),
                 error=command_error(error))
        raise
    finally:
        shutil.rmtree(snapshot)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("state", type=Path)
    parser.add_argument("codex")
    parser.add_argument("running_build")
    parser.add_argument("--worker", type=int)
    arguments = parser.parse_args()
    if arguments.worker is not None:
        with os.fdopen(arguments.worker, "a"):
            work(arguments.state, arguments.codex, arguments.running_build, arguments.worker)
    else:
        os.umask(0o077)
        os.environ["GIT_TERMINAL_PROMPT"] = "0"
        config_home = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")
        gitconfig = config_home / "codex-session-board/gitconfig"
        if gitconfig.is_file():
            os.environ["GIT_CONFIG_GLOBAL"] = str(gitconfig)
        start(arguments.state, arguments.codex, arguments.running_build)


if __name__ == "__main__":
    main()
