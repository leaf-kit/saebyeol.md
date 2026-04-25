#!/usr/bin/env bash
# 새별 마크다운 에디터 (sbmd) 프로젝트 관리 스크립트 (메뉴형)
#
# 사용법:
#   ./manage.sh            # 대화형 메뉴 실행
#   ./manage.sh <번호|키>  # 메뉴 항목 직접 실행 (예: ./manage.sh dev)

set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

# ---------- 색상 ----------
if [[ -t 1 ]]; then
    C_RESET=$'\033[0m'
    C_BOLD=$'\033[1m'
    C_DIM=$'\033[2m'
    C_GREEN=$'\033[32m'
    C_YELLOW=$'\033[33m'
    C_BLUE=$'\033[34m'
    C_CYAN=$'\033[36m'
    C_RED=$'\033[31m'
else
    C_RESET='' C_BOLD='' C_DIM='' C_GREEN='' C_YELLOW='' C_BLUE='' C_CYAN='' C_RED=''
fi

log()    { printf '%s[leaf]%s %s\n' "$C_CYAN" "$C_RESET" "$*"; }
ok()     { printf '%s[ ok ]%s %s\n' "$C_GREEN" "$C_RESET" "$*"; }
warn()   { printf '%s[warn]%s %s\n' "$C_YELLOW" "$C_RESET" "$*"; }
err()    { printf '%s[err ]%s %s\n' "$C_RED" "$C_RESET" "$*" >&2; }

run() {
    printf '%s$%s %s\n' "$C_DIM" "$C_RESET" "$*"
    "$@"
}

need() {
    if ! command -v "$1" >/dev/null 2>&1; then
        err "필요한 명령어가 없습니다: $1"
        [[ -n "${2:-}" ]] && warn "설치 힌트: $2"
        return 1
    fi
}

pause() {
    printf '\n%s계속하려면 Enter 키를 누르세요...%s' "$C_DIM" "$C_RESET"
    read -r _
}

# ---------- 내부 헬퍼 ----------
has_tauri_cli() {
    command -v cargo-tauri >/dev/null 2>&1 || cargo tauri --version >/dev/null 2>&1
}

tauri() {
    if has_tauri_cli; then
        run cargo tauri "$@"
    else
        warn "tauri CLI 가 설치되어 있지 않습니다. 'cargo run -p sb-md' 로 대체합니다."
        warn "(정식 dev/build 기능을 쓰려면 메뉴 [11] tauri CLI 설치 를 실행하세요.)"
        if [[ "${1:-}" == "dev" ]]; then
            run cargo run -p sb-md
        elif [[ "${1:-}" == "build" ]]; then
            run cargo build -p sb-md --release
        else
            err "tauri CLI 가 없어 '$*' 를 실행할 수 없습니다."
            return 1
        fi
    fi
}

# ---------- 액션 ----------
action_dev() {
    log "Tauri 앱을 dev 모드로 실행합니다."
    need cargo "https://rustup.rs" || return 1
    tauri dev
}

action_demo() {
    log "터미널 IME 데모를 실행합니다 (Ctrl-C 종료)."
    need cargo || return 1
    run cargo run -p ime-demo
}

action_run_app() {
    log "Tauri 앱을 debug 바이너리로 바로 실행합니다."
    need cargo || return 1
    run cargo run -p sb-md
}

action_build_app() {
    log "Tauri 앱을 release 로 번들링합니다."
    need cargo || return 1
    tauri build
}

action_build_release() {
    log "전체 워크스페이스를 release 로 빌드합니다."
    need cargo || return 1
    run cargo build --workspace --release
}

action_test() {
    log "워크스페이스 테스트를 실행합니다."
    need cargo || return 1
    run cargo test --workspace
}

action_check() {
    log "cargo check (모든 타겟) 을 실행합니다."
    need cargo || return 1
    run cargo check --workspace --all-targets
}

action_clippy() {
    log "clippy 린트를 실행합니다."
    need cargo || return 1
    run cargo clippy --workspace --all-targets -- -D warnings
}

action_fmt() {
    log "rustfmt 으로 코드를 포맷합니다."
    need cargo || return 1
    run cargo fmt --all
}

action_fmt_check() {
    log "rustfmt 포맷 검사 (수정 없이 차이만 보고)."
    need cargo || return 1
    run cargo fmt --all -- --check
}

action_clean() {
    log "target/ 을 정리합니다."
    need cargo || return 1
    run cargo clean
}

action_update() {
    log "Cargo.lock 의 의존성을 업데이트합니다."
    need cargo || return 1
    run cargo update
}

action_doc() {
    log "rustdoc 문서를 생성하고 브라우저로 엽니다."
    need cargo || return 1
    run cargo doc --workspace --no-deps --open
}

action_install_tauri() {
    log "tauri CLI (cargo-tauri) 를 설치합니다."
    need cargo || return 1
    run cargo install tauri-cli --version '^2.0' --locked
}

action_ci() {
    log "CI 파이프라인(fmt-check → clippy → test) 을 순차 실행합니다."
    need cargo || return 1
    action_fmt_check && action_clippy && action_test && ok "모든 CI 단계 통과"
}

action_status() {
    log "프로젝트 상태 요약"
    echo
    printf '  %s루트:%s %s\n' "$C_BOLD" "$C_RESET" "$ROOT_DIR"
    if command -v cargo >/dev/null 2>&1; then
        printf '  %scargo:%s %s\n' "$C_BOLD" "$C_RESET" "$(cargo --version)"
    else
        printf '  %scargo:%s %s없음%s\n' "$C_BOLD" "$C_RESET" "$C_RED" "$C_RESET"
    fi
    if has_tauri_cli; then
        printf '  %stauri:%s %s\n' "$C_BOLD" "$C_RESET" "$(cargo tauri --version 2>/dev/null || echo '설치됨')"
    else
        printf '  %stauri:%s %s미설치%s (메뉴 11 에서 설치)\n' "$C_BOLD" "$C_RESET" "$C_YELLOW" "$C_RESET"
    fi
    printf '  %s워크스페이스 멤버:%s\n' "$C_BOLD" "$C_RESET"
    printf '    - apps/ime-app/src-tauri   (Tauri 데스크톱 앱 · 크레이트명 sb-md)\n'
    printf '    - crates/ime-demo          (터미널 데모 바이너리)\n'
    printf '    - crates/lib-ime           (IME 코어 라이브러리)\n'
    if [[ -d target ]]; then
        local size
        size=$(du -sh target 2>/dev/null | awk '{print $1}')
        printf '  %starget/ 크기:%s %s\n' "$C_BOLD" "$C_RESET" "$size"
    fi
}

# ---------- 메뉴 ----------
print_menu() {
    cat <<EOF

${C_BOLD}${C_GREEN}╭──────────────────────────────────────────────╮
│            sbmd — Manage Console             │
╰──────────────────────────────────────────────╯${C_RESET}

  ${C_BOLD}실행${C_RESET}
    ${C_CYAN}1${C_RESET})  dev           Tauri 앱 dev 모드 (hot-reload)
    ${C_CYAN}2${C_RESET})  demo          터미널 IME 데모 실행
    ${C_CYAN}3${C_RESET})  run-app       Tauri 앱 debug 실행 (dev 서버 없이)

  ${C_BOLD}빌드${C_RESET}
    ${C_CYAN}4${C_RESET})  build-app     Tauri 앱 release 번들링
    ${C_CYAN}5${C_RESET})  build         워크스페이스 release 빌드

  ${C_BOLD}품질${C_RESET}
    ${C_CYAN}6${C_RESET})  test          cargo test (워크스페이스)
    ${C_CYAN}7${C_RESET})  check         cargo check --all-targets
    ${C_CYAN}8${C_RESET})  clippy        clippy (-D warnings)
    ${C_CYAN}9${C_RESET})  fmt           rustfmt 적용
   ${C_CYAN}10${C_RESET})  fmt-check     rustfmt 검사만
   ${C_CYAN}14${C_RESET})  ci            fmt-check + clippy + test

  ${C_BOLD}유틸${C_RESET}
   ${C_CYAN}11${C_RESET})  install-tauri tauri CLI 설치 (cargo-tauri)
   ${C_CYAN}12${C_RESET})  clean         target/ 정리
   ${C_CYAN}13${C_RESET})  update        cargo update
   ${C_CYAN}15${C_RESET})  doc           rustdoc 생성 + 열기
   ${C_CYAN}16${C_RESET})  status        환경/프로젝트 상태

    ${C_CYAN}0${C_RESET})  quit          종료
EOF
}

dispatch() {
    case "${1:-}" in
        1|dev)                 action_dev ;;
        2|demo)                action_demo ;;
        3|run-app|run)         action_run_app ;;
        4|build-app|bundle)    action_build_app ;;
        5|build)               action_build_release ;;
        6|test|t)              action_test ;;
        7|check|c)             action_check ;;
        8|clippy|lint)         action_clippy ;;
        9|fmt|format)          action_fmt ;;
        10|fmt-check)          action_fmt_check ;;
        11|install-tauri)      action_install_tauri ;;
        12|clean)              action_clean ;;
        13|update)             action_update ;;
        14|ci)                 action_ci ;;
        15|doc|docs)           action_doc ;;
        16|status|info)        action_status ;;
        0|q|quit|exit)         return 2 ;;
        '')                    return 3 ;;
        h|help|-h|--help)
            print_menu
            return 0
            ;;
        *)
            err "알 수 없는 항목: $1"
            return 1
            ;;
    esac
}

main() {
    if [[ $# -gt 0 ]]; then
        dispatch "$1"
        exit $?
    fi

    while true; do
        print_menu
        printf '\n%s선택%s [0-16]: ' "$C_BOLD" "$C_RESET"
        read -r choice || { echo; break; }

        dispatch "$choice"
        rc=$?
        if [[ $rc -eq 2 ]]; then
            log "종료합니다."
            break
        fi
        if [[ $rc -eq 3 ]]; then
            continue
        fi

        pause
    done
}

main "$@"
