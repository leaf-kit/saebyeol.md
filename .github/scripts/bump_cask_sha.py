"""Casks/saebyeol.rb 의 sha256 블록을 arm/intel 두 값으로 갱신한다.

release.yml 워크플로의 bump-cask-sha 잡에서 호출되며, 환경변수
ARM_SHA · INTEL_SHA 를 입력으로 받아 cask 파일을 in-place 수정한다.

기존 sha256 블록은 다음 두 형태 중 하나일 수 있어 둘 다 대응한다:

    sha256 "단일값"

    sha256 arm:   "값1",
           intel: "값2"
"""

from __future__ import annotations

import os
import pathlib
import re
import sys


def main() -> int:
    arm = os.environ.get("ARM_SHA", "")
    intel = os.environ.get("INTEL_SHA", "")
    if not (arm and intel):
        print(f"::error::ARM_SHA / INTEL_SHA 가 비어 있음 (arm={arm!r} intel={intel!r})")
        return 1

    cask = pathlib.Path("Casks/saebyeol.rb")
    content = cask.read_text()

    new_block = f'  sha256 arm:   "{arm}",\n         intel: "{intel}"'

    multi = re.compile(r"^  sha256 arm:.*\n^\s+intel:[^\n]*$", re.M)
    single = re.compile(r'^  sha256 "[^"]*"$', re.M)

    if multi.search(content):
        content = multi.sub(new_block, content, count=1)
    elif single.search(content):
        content = single.sub(new_block, content, count=1)
    else:
        print("::error::Casks/saebyeol.rb 에 sha256 블록을 찾을 수 없음")
        return 1

    cask.write_text(content)
    print(f"updated cask sha256: arm={arm[:12]}… intel={intel[:12]}…")
    return 0


if __name__ == "__main__":
    sys.exit(main())
