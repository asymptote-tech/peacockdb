"""docker/cmake-wrapper.sh: which configures get the prefetched rapids-cmake and the flatbuffers
mirror, and that every other cmake mode is untouched.

The wrapper's two path constants are rewritten in a copy to a fake cmake, which records its argv
and the git config the environment carries, and to a fake prefetched copy.
"""
import json
import os
import pathlib
import subprocess
import sys
import tempfile

sys.path.append(str(pathlib.Path(__file__).resolve().parents[2] / "exec_model" / "tests"))
from harness import main  # noqa: E402

WRAPPER = pathlib.Path(__file__).resolve().parents[3] / "docker" / "cmake-wrapper.sh"
REAL, COPY = "real=/opt/cmake/bin/cmake", "copy=/opt/prefetch/rapids-cmake"
MIRROR = "url.file:///opt/prefetch/flatbuffers.git.insteadOf"
FLATBUFFERS = "https://github.com/google/flatbuffers.git"
OVERRIDE = "-DFETCHCONTENT_SOURCE_DIR_RAPIDS-CMAKE="

FAKE_CMAKE = r'''#!/usr/bin/env python3
import json, os, sys
n = int(os.environ.get("GIT_CONFIG_COUNT", "0"))
config = [[os.environ.get(f"GIT_CONFIG_KEY_{i}"), os.environ.get(f"GIT_CONFIG_VALUE_{i}")] for i in range(n)]
with open(os.environ["FAKE_CMAKE_LOG"], "w") as f:
    json.dump({"argv": sys.argv[1:], "git_config": config}, f)
'''


class World:
    """A temp dir: the wrapper with its constants pointed here, a fake cmake, and a source tree."""

    def __init__(self, tree_branch="branch-25.10", copy_branch="branch-25.10"):
        self.tmp = tempfile.TemporaryDirectory()
        root = pathlib.Path(self.tmp.name)
        text = WRAPPER.read_text()
        assert REAL in text and COPY in text, "the wrapper's path constants moved; update this test"
        self.copy = root / "prefetch" / "rapids-cmake"
        self.copy.mkdir(parents=True)
        (root / "prefetch" / "rapids-cmake.branch").write_text(copy_branch + "\n")
        fake = root / "bin" / "cmake"
        fake.parent.mkdir()
        fake.write_text(FAKE_CMAKE)
        fake.chmod(0o755)
        self.wrapper = root / "cmake"
        self.wrapper.write_text(text.replace(REAL, f"real={fake}").replace(COPY, f"copy={self.copy}"))
        self.wrapper.chmod(0o755)
        self.src = root / "tree" / "cpp"
        self.src.mkdir(parents=True)
        (self.src / "CMakeLists.txt").write_text("project(t NONE)\n")
        if tree_branch is not None:
            (root / "tree" / "third_party" / "cudf").mkdir(parents=True)
            (root / "tree" / "third_party" / "cudf" / "RAPIDS_BRANCH").write_text(tree_branch + "\n")
        self.build = root / "build"
        self.build.mkdir()
        self.log = root / "cmake.log"

    def run(self, *args, env=None):
        full = {**os.environ, "FAKE_CMAKE_LOG": str(self.log), **(env or {})}
        for name in [k for k in full if k.startswith("GIT_CONFIG_")]:
            if not (env and name in env):
                del full[name]
        proc = subprocess.run([str(self.wrapper), *map(str, args)], capture_output=True, text=True,
                              env=full, cwd=self.build, timeout=30)
        assert proc.returncode == 0, proc.stderr
        return json.loads(self.log.read_text())

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.tmp.cleanup()


def overrides(argv):
    return [a for a in argv if a.startswith(OVERRIDE)]


def test_a_matching_branch_gets_the_copy_first_whatever_names_the_source():
    with World() as w:
        for args in (["-S", w.src, "-B", w.build], [f"-S{w.src}", "-B", w.build], [w.src, "-G", "Unix Makefiles"]):
            argv = w.run(*args)["argv"]
            assert argv[0] == f"{OVERRIDE}{w.copy}" and argv[1:] == list(map(str, args)), argv


def test_a_later_user_define_comes_after_the_copy():
    with World() as w:
        mine = f"{OVERRIDE}/elsewhere"
        argv = w.run("-S", w.src, mine)["argv"]
        assert overrides(argv) == [f"{OVERRIDE}{w.copy}", mine], argv


def test_a_different_branch_clears_a_cached_copy():
    with World(tree_branch="branch-99.99") as w:
        argv = w.run("-S", w.src, "-B", w.build)["argv"]
        assert argv[0] == OVERRIDE and overrides(argv) == [OVERRIDE], argv


def test_a_tree_without_rapids_branch_is_left_alone():
    with World(tree_branch=None) as w:
        argv = w.run("-S", w.src)["argv"]
        assert overrides(argv) == [] and argv == ["-S", str(w.src)], argv


def test_option_values_are_not_taken_for_the_source():
    with World() as w:
        decoy = w.src.parent / "decoy"  # beside cpp/, so taken for the source it would match
        decoy.mkdir()
        (decoy / "CMakeLists.txt").write_text("project(d NONE)\n")
        for flag in ("-B", "-G", "-C", "-T", "-A", "-D", "-U"):
            argv = w.run(flag, decoy)["argv"]
            assert overrides(argv) == [], (flag, argv)


def test_other_modes_pass_through_untouched():
    with World() as w:
        for args in (["--build", w.build], ["--install", w.build], ["-E", "echo", "x"], ["-P", "s.cmake"], ["--version"]):
            seen = w.run(*args)
            assert seen == {"argv": list(map(str, args)), "git_config": []}, (args, seen)


def test_a_configure_points_flatbuffers_at_the_mirror():
    with World(tree_branch=None) as w:
        assert w.run("-S", w.src)["git_config"] == [[MIRROR, FLATBUFFERS]]


def test_the_mirror_is_appended_to_git_config_already_set():
    with World() as w:
        env = {"GIT_CONFIG_COUNT": "1", "GIT_CONFIG_KEY_0": "safe.directory", "GIT_CONFIG_VALUE_0": "*"}
        assert w.run("-S", w.src, env=env)["git_config"] == [["safe.directory", "*"], [MIRROR, FLATBUFFERS]]


if __name__ == "__main__":
    raise SystemExit(main(globals()))
