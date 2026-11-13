"""install-watch ships files from this tree; the fake rsync only records, so check the sources exist."""
import pathlib, subprocess, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fakes import World  # noqa: E402
from harness import main  # noqa: E402

SCRIPTS = pathlib.Path(__file__).resolve().parents[2]


def test_install_watch_ships_files_that_exist():
    w = World(state="RUNNING")
    p = subprocess.run([str(SCRIPTS / "nebius.sh"), "install-watch"], env=w.env(),
                       capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL)
    assert p.returncode == 0, (p.returncode, p.stderr)
    # Every argument but -e's value, the transport, which names this tree's nebius.sh too.
    ours = [a for c in w.read()["calls"] if c[0] == "rsync"
            for i, a in enumerate(c) if i > 0 and c[i - 1] != "-e" and a.startswith(str(SCRIPTS))]
    assert any(a.endswith("/idle_watch.py") for a in ours), ours
    missing = [a for a in ours if not pathlib.Path(a).exists()]
    assert not missing, missing


if __name__ == "__main__":
    raise SystemExit(main(globals()))
