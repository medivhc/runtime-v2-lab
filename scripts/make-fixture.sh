#!/usr/bin/env bash
# make-fixture.sh — 生成 Runtime 试件仓库（规格附录 B 的自动化版）
# 输出: fixture 路径、bare 仓库路径、基线 SHA（stdout），并写入 $LAB/secrets/fixture.env
set -euo pipefail
LAB=${LAB:-/root/workspace/runtime-v2-lab}
ROOT_DIR=${1:-$LAB/fixture}
mkdir -p "$LAB" "$ROOT_DIR"
FIXTURE="$(cd "$ROOT_DIR" && pwd -P)"
cd "$FIXTURE"
git init -q 2>/dev/null
git config user.name 'Runtime Fixture'
git config user.email 'runtime-fixture@example.invalid'
cat > counter.py <<'PY'
def increment(value):
    return value
PY
cat > test_counter.py <<'PY'
import unittest
from counter import increment

class CounterTest(unittest.TestCase):
    def test_increment(self):
        for value in (-1, 0, 5):
            self.assertEqual(increment(value), value + 1)
PY
git add counter.py test_counter.py
git commit -qm 'Seed failing increment fixture'
if PYTHONDONTWRITEBYTECODE=1 python3 -m unittest -v test_counter 2>/dev/null; then
  echo 'Invalid fixture: expected failure' >&2
  exit 1
fi
git clone -q --bare "$FIXTURE" "$FIXTURE.git"
BASELINE=$(git rev-parse HEAD)
printf 'fixture=%s bare=%s baseline=%s\n' "$FIXTURE" "$FIXTURE.git" "$BASELINE"
umask 077
cat > "$LAB/secrets/fixture.env" <<EOF
FIXTURE=$FIXTURE
FIXTURE_BARE=$FIXTURE.git
BASELINE=$BASELINE
EOF
