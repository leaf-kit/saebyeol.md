<div align="center">

# 새별 · Saebyeol

**한글 IME 가 내장된 마크다운 에디터**

[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Tauri](https://img.shields.io/badge/Tauri-2.x-24C8DB?logo=tauri&logoColor=white)](https://tauri.app)
[![License](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-macOS-lightgrey?logo=apple&logoColor=white)](#install)
[![Version](https://img.shields.io/badge/Version-v0.1.0-blue.svg)](#)
[![Code Style](https://img.shields.io/badge/Lints-pedantic%20%2B%20deny%20unsafe-purple.svg)](#)
[![Homepage](https://img.shields.io/badge/Homepage-leaf--kit.github.io%2Fsaebyeol-success?logo=github)](https://leaf-kit.github.io/saebyeol/)

</div>

---

## 한 줄 소개

세벌식 최종·두벌식·모아치기를 모두 지원하는 자체 한글 IME 위에서 동작하는, Rust + Tauri 기반의 가벼운 macOS 마크다운 에디터.

> **자세한 소개·스크린샷·사용 가이드는 공식 페이지에서 확인하세요 →
> https://leaf-kit.github.io/saebyeol/**

---

## 용어 (Terminology)

| 용어 | 설명 |
|------|------|
| **새별** | 한글 브랜드명. 본 앱의 정식 한국어 명칭이다. |
| **Saebyeol / sb-md / sbmd** | 새별의 ASCII 별칭. 바이너리·크레이트·번들 식별자에 사용한다. |
| **lib-ime** | IME 코어 라이브러리. 자모(`Jamo`), 한글 FSM, 자판, 자동완성을 모두 담는다. |
| **모아치기** | 한 음절을 이루는 자모를 거의 동시에 눌러 입력하는 방식. |
| **세벌식 최종** | 한국 3-set 표준 자판. 초성·중성·종성을 분리 키로 입력한다. |
| **두벌식** | 한국 2-set 표준 자판. 초성·종성 자음이 같은 키를 공유한다. |
| **약어 (Abbreviation)** | 짧은 트리거(예: 초성 시퀀스)로 긴 본문을 자동 입력하는 기능. |
| **n-gram 학습** | 사용자 마크다운 폴더를 스캔해 빈출 어절을 약어 사전으로 추출. |

---

## 특징

- **자체 한글 IME 코어** — 두벌식·세벌식 최종·세벌식 390 + Latin(QWERTY/Dvorak), 모아치기/순차 입력, NFC 음절·연접 자모 출력 형식 선택.
- **마크다운 편집기** — 라이브 미리보기, 탭 기반 다중 문서, 사이드바 파일 트리, 13종 테마(다크 6 + 라이트 7).
- **자동완성** — 내장 시드 사전 + 사용자 `abbreviations.toml` + n-gram 학습 결과(`learned_ngrams.toml`) 를 병합.
- **저장 안전성** — 창 X 버튼·⌘Q·Dock 종료 어느 경로든 가로채, 저장 안 된 탭을 **개별** 단위로 저장 여부 확인.
- **설정 초기화 안전망** — 보존되는 항목(열린 탭·사용자 사전·학습 사전·디스크 파일)과 예외(에디터 폭 권장값 880px 재설정 등) 를 모달에 모두 명시.
- **속도와 안전성** — `unsafe_code = "forbid"`, clippy pedantic, release `lto = "thin"` + `codegen-units = 1`.

---

## Install

### Homebrew Cask (권장)

```bash
brew tap leaf-kit/saebyeol.md
brew install --cask saebyeol
```

### 업데이트

```bash
brew update
brew upgrade --cask saebyeol
```

### 제거

```bash
brew uninstall --cask saebyeol
brew untap leaf-kit/saebyeol.md
```

### 소스에서 빌드

```bash
git clone https://github.com/leaf-kit/saebyeol.md
cd saebyeol.md
cargo install tauri-cli --version '^2.0' --locked
./manage.sh build-app   # release 번들 생성 (.app)
```

### 첫 실행 시 macOS 보안 경고 (1회만)

새별은 아직 Apple Developer ID 코드사이닝·노터리제이션 전이라 첫 실행 시 macOS Gatekeeper 가 *"Apple은 '새별.app' … 악성 코드가 없음을 확인할 수 없습니다"* 경고를 띄웁니다. **앱은 ad-hoc 서명되어 있어 한 번만 승인하면 이후엔 더블클릭으로 바로 실행됩니다.**

승인 방법 (둘 중 어느 쪽이든 OK):

1. **권장 — 시스템 설정에서 "그래도 열기"**
   1. 앱을 한 번 더블클릭 → 차단 경고 → **확인** 으로 닫기.
   2. **시스템 설정 → 개인정보 보호 및 보안** 으로 이동.
   3. 스크롤 맨 아래 *"새별.app 을(를) 차단했지만 그래도 열기"* 메시지 옆 **그래도 열기** 클릭.
   4. 다시 한 번 macOS 비밀번호 입력 → 앱이 정상 실행. 이후 더블클릭만으로 동작.

2. **우클릭 → 열기 (Finder)**
   - Finder 에서 `새별.app` **우클릭 → 열기** → 경고창에서 **열기** 클릭. 1회만 하면 됨.

영구적으로 모든 사용자에게 경고가 뜨지 않게 하려면 Apple Developer Program (연 $99) 가입 후 인증서 발급·서명·노터리제이션이 필요합니다. 절차는 아래 "릴리스 절차" 절 참조.

---

## 빠른 시작

대화형 메뉴:

```bash
./manage.sh
```

자주 쓰는 단축 명령:

```bash
./manage.sh dev          # Tauri 앱 dev 모드 (hot-reload)
./manage.sh demo         # 터미널 IME 데모
./manage.sh test         # cargo test --workspace
./manage.sh check        # cargo check --all-targets
./manage.sh clippy       # clippy -D warnings
./manage.sh fmt          # rustfmt 적용
./manage.sh ci           # fmt-check + clippy + test 일괄 실행
./manage.sh build-app    # Tauri 앱 release 번들링
./manage.sh status       # 환경 / 프로젝트 상태 요약
```

---

## 프로젝트 구조

```
saebyeol.md/
├── apps/
│   └── ime-app/
│       ├── dist/                  # 프런트엔드 자산 (index.html · script.js · style.css)
│       └── src-tauri/             # Tauri 데스크톱 셸
│           ├── src/main.rs        # IPC · 메뉴 · 종료 흐름
│           ├── src/settings.rs    # settings.toml 영속화
│           ├── src/ngram.rs       # n-gram 학습 산출
│           ├── capabilities/      # Tauri 권한 매니페스트
│           ├── icons/             # 앱 아이콘 (macOS · iOS · Android · Windows)
│           ├── tauri.conf.json    # 윈도우/번들 정의
│           └── Cargo.toml
├── crates/
│   ├── lib-ime/                   # IME 코어 라이브러리
│   │   ├── src/hangul/            # 자모 · FSM · 음절 합성 · 출력 변환
│   │   ├── src/layout/            # 두벌식 · 세벌식(최종/390) · QWERTY · Dvorak · 사용자 정의
│   │   └── src/abbr/              # 자동완성 엔진 · 사전 로더
│   └── ime-demo/                  # 터미널 IME 데모 바이너리
├── manage.sh                      # 빌드/테스트/실행 통합 스크립트 (메뉴형)
├── Cargo.toml                     # 워크스페이스 매니페스트
└── LICENSE                        # MIT OR Apache-2.0
```

---

## 설정 파일 위치 (macOS)

| 파일 | 경로 |
|------|------|
| 앱 설정 | `~/Library/Application Support/dev.leaf.sbmd/settings.toml` |
| 사용자 자동완성 사전 | `~/Library/Application Support/dev.leaf.sbmd/abbreviations.toml` |
| 학습된 n-gram 사전 | `~/Library/Application Support/dev.leaf.sbmd/learned_ngrams.toml` |

`설정 → 설정 초기화` 는 위 `settings.toml` 만 삭제하며, 자동완성 사전 두 파일과 열린 탭은 보존된다.

---

## 요구사항

- Rust 1.75 이상 (`rustup` 권장)
- macOS 11 (Big Sur) 이상
- Tauri CLI 2.x — `cargo install tauri-cli --version '^2.0' --locked`

---

## 버전 정책

`v<MAJOR>.<MINOR>.<PATCH>` 의 [SemVer](https://semver.org/lang/ko/) 를 따른다. 현재 버전은 [`Cargo.toml`](Cargo.toml) 의 `[workspace.package].version` 에서 단일 출처로 관리하며, 변경 내역은 [`CHANGELOG.md`](CHANGELOG.md) 에 기록한다. release 워크플로가 새 태그의 CHANGELOG 섹션을 자동 추출해 GitHub Release 본문으로 사용한다.

---

## 자동 업데이트 (in-app)

설치된 앱은 [Tauri Updater](https://v2.tauri.app/plugin/updater/) 로 새 버전을 감지한다.

- **부팅 직후 4초 뒤** 백그라운드로 `https://github.com/leaf-kit/saebyeol.md/releases/latest/download/latest.json` 매니페스트를 조회.
- 새 버전이 있으면 모달로 `현재 → 신규`, 릴리스 노트, **지금 설치 / 나중에** 안내. "지금 설치" 시 다운로드·서명 검증·교체 후 자동 재시작.
- 사용자가 직접 확인하려면 메뉴 `새별 → 업데이트 확인…`.
- 매니페스트는 GitHub Release 자산의 일부로 [`tauri-action`](https://github.com/tauri-apps/tauri-action) 이 자동 생성한다 (`includeUpdaterJson: true`).

---

## 릴리스 절차 (배포자용)

### 1. 서명 키 1회 생성

```bash
cargo install tauri-cli --version '^2.0' --locked   # 최초 1회
cargo tauri signer generate -w ~/.tauri/saebyeol.key
```

생성된 **공개키** 를 `apps/ime-app/src-tauri/tauri.conf.json` 의 `plugins.updater.pubkey` 자리표시자와 교체한다.

### 2. Apple Developer ID 인증서 (.p12) 준비

배포 머신의 **Keychain Access** 에서 본인의 `Developer ID Application` 인증서를 `.p12` 로 export 한다 (인증서 행을 펼쳐 개인 키와 함께 선택 → 우클릭 → 내보내기 → "개인 정보 교환(.p12)" → 비밀번호 설정).

```bash
# export 한 .p12 를 base64 한 줄로 인코딩
base64 -i ~/Desktop/developerID_application.p12 -o ~/Desktop/cert.p12.b64
cat ~/Desktop/cert.p12.b64 | pbcopy   # 클립보드로
```

인증서의 정확한 식별 문자열(CN) 은 다음으로 확인:

```bash
security find-identity -v -p codesigning | grep "Developer ID Application"
# 예: 1) ABCD…  "Developer ID Application: <이름> (<TEAMID>)"
```

### 3. GitHub Secrets 등록

저장소 `Settings → Secrets and variables → Actions` 에서:

| 이름 | 값 |
|------|------|
| `TAURI_SIGNING_PRIVATE_KEY` | `~/.tauri/saebyeol.key` 파일 내용 (앱 내 자동 업데이트 매니페스트 서명용) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 키 생성 시 입력한 비밀번호 |
| `APPLE_CERTIFICATE` | `cert.p12.b64` 의 내용 (앞 단계 결과) |
| `APPLE_CERTIFICATE_PASSWORD` | `.p12` export 시 정한 비밀번호 |
| `APPLE_SIGNING_IDENTITY` | `security find-identity` 로 확인한 인증서 CN. 워크플로 env 가 이 값을 그대로 tauri 에 넘겨 `tauri.conf.json` 의 `signingIdentity: "-"` (ad-hoc 기본값) 를 override 한다. |
| `APPLE_ID` | Apple Developer 계정 이메일 |
| `APPLE_PASSWORD` | https://appleid.apple.com → Sign-In and Security → App-Specific Passwords 에서 생성한 16자리 비밀번호 |
| `APPLE_TEAM_ID` | Apple Developer 계정의 10자리 Team ID |

추가로 [별도 tap 저장소](https://github.com/leaf-kit/homebrew-saebyeol.md) 자동 미러링을 활성화하려면:

| `TAP_GITHUB_TOKEN` | `leaf-kit/homebrew-saebyeol.md` 에 push 권한이 있는 fine-grained PAT |

### 4. 태그 푸시

```bash
git tag -a v0.1.1 -m "v0.1.1"
git push origin v0.1.1
```

`.github/workflows/release.yml` 이 자동으로:

1. arm64 · x86_64 양 아키텍처로 release 빌드.
2. Apple Developer ID 인증서로 `.app` 코드사이닝 + Apple 노터리 서비스 등록 + staple.
3. `.dmg`, `.app.tar.gz` 등을 GitHub Release 에 업로드.
4. `Casks/saebyeol.rb` 의 버전·arch별 SHA256 을 자동 갱신해 main 에 커밋. `TAP_GITHUB_TOKEN` 시크릿이 있으면 별도 tap 저장소도 동시에 갱신.

릴리스 후엔 기존 사용자가 다음 실행 시점부터 자동으로 새 버전을 안내받고, 첫 실행 Gatekeeper 경고도 사라진다.

---

## 라이선스

본 프로젝트는 다음 두 라이선스 중 사용자가 선택할 수 있는 듀얼 라이선스로 배포된다:

- [MIT License](LICENSE)
- Apache License 2.0

---

<div align="center">
<sub>Built with Rust · Tauri · 한글</sub>
</div>
