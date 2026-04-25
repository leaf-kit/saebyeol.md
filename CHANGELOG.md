# 변경 이력

본 파일은 [Keep a Changelog](https://keepachangelog.com/ko/1.1.0/) 형식을 따르며,
버전은 [SemVer](https://semver.org/lang/ko/) 를 따른다. 모든 항목은 한글로 작성한다.

## v0.1.1 — 2026-04-26

v0.1.0 의 배포 인프라·UX 를 다듬은 패치 릴리스. 기능 변경은 없고 설치·실행 경험이 매끄러워진다.

### 한국어 앱 표시명
- macOS `.app` 폴더와 Dock·Finder·About 메뉴 모두 **새별** 으로 통일 (`productName: "새별"`).
- 메뉴·창 제목·상태표시줄의 한국어 표기는 기존대로.

### Apple Developer ID 코드사이닝·노터리제이션
- `.app` 을 Developer ID 인증서로 정식 서명한 뒤 Apple 노터리 서비스에 등록·staple.
- 첫 실행 Gatekeeper 경고가 사라지고 더블클릭만으로 바로 실행됨 (배포자가 `APPLE_*` GitHub Secrets 를 등록한 경우).
- Hardened Runtime entitlements (`Entitlements.plist`) 최소 권한만 부여 (JIT · unsigned executable memory · disable library validation).

### Homebrew Cask 안정화
- 자산 무결성 검사를 위해 cask 의 `sha256` 을 `arm:` / `intel:` 두 값으로 분리. `bump-cask-sha` 잡이 새 태그마다 양쪽을 자동 갱신.
- `latest.json` 매니페스트가 release 자산 일부로 게시되어 자동 업데이트 확인 메뉴가 정상 동작 ("최신 버전" 안내).
- 별도 tap 저장소 [`leaf-kit/homebrew-saebyeol.md`](https://github.com/leaf-kit/homebrew-saebyeol.md) 와 메인 저장소 cask 를 워크플로가 동시 미러링.

### 릴리스 노트 자동화
- `CHANGELOG.md` 의 해당 버전 섹션을 워크플로가 자동 추출해 GitHub Release 본문으로 사용 (`.github/scripts` 와 `.github/release-body-suffix.md`).

[v0.1.1]: https://github.com/leaf-kit/saebyeol.md/releases/tag/v0.1.1

## v0.1.0 — 2026-04-25

새별 마크다운 에디터의 첫 정식 공개. 한글 IME 가 내장된 Rust + Tauri 기반 macOS 마크다운 에디터.

### 한글 IME 코어 (lib-ime)
- 두벌식 / 세벌식 최종 / 세벌식 390 / Latin(QWERTY · Dvorak) 자판 지원.
- 모아치기 + 순차 입력 모드 전환.
- NFC 음절 / 연접 자모(conjoining) 출력 형식 선택.
- 자모 FSM 기반 합성으로 빠른 키 응답과 안전한 조합 상태 복구.
- 자동완성 엔진: 내장 시드 사전 + 사용자 사전(`abbreviations.toml`) + n-gram 학습 사전(`learned_ngrams.toml`) 병합.

### 마크다운 에디터 (sb-md · 새별)
- 라이브 인라인 마크다운 렌더 (헤딩 · 강조 · 인용 · 표 · 코드 펜스 · 링크 · 이미지 · 수식 · 알림 · 각주 · TOC · YAML front-matter).
- 탭 기반 다중 문서 + 사이드바 파일 트리 + 핀 고정.
- 13종 테마 (다크 6 + 라이트 7).
- 풍부한 단축키와 한글 메뉴, 단락별 변환, 자동 들여쓰기.
- 자동완성 UI · 모양 설정 · 줌 · 항상 위에 등 사용성 옵션.

### 종료 · 저장 안전망
- 창 X 버튼 · ⌘Q · Dock 종료 모두 가로채, 저장 안 된 탭이 있으면 사용자에게 확인.
- 탭별로 개별 저장 여부 결정 (활성 탭 자동 전환으로 어느 문서인지 시각적으로 안내).
- 설정 초기화 시 보존되는 항목(열린 탭 · 사용자 사전 · 학습 사전 · 디스크 파일) 과 예외(에디터 폭 권장값 880px 재설정 등) 를 모달에 모두 명시.

### 배포 · 자동 업데이트
- macOS Apple Silicon(arm64) · Intel(x86_64) dmg 번들.
- Homebrew Cask 배포 — `brew tap leaf-kit/saebyeol.md && brew install --cask saebyeol`.
- Tauri Updater + GitHub Releases `latest.json` 매니페스트로 앱 내 자동 업데이트 안내.
- `.github/workflows/release.yml` 로 태그 push 시 빌드 → 서명 → 릴리스 → cask SHA 자동 갱신.
- 별도 tap 저장소 [`leaf-kit/homebrew-saebyeol.md`](https://github.com/leaf-kit/homebrew-saebyeol.md) 운영, 메인 저장소와 자동 동기화.

### 안전성 · 품질
- `unsafe_code = "forbid"`, clippy `pedantic`.
- release 프로파일 `lto = "thin"`, `codegen-units = 1`.
- 워크스페이스 단위 테스트(134 + 11 + 12 + 1 doc) CI 게이트.

[v0.1.0]: https://github.com/leaf-kit/saebyeol.md/releases/tag/v0.1.0
