
---

## 설치

### macOS — Homebrew Cask (권장)

```bash
brew tap leaf-kit/saebyeol.md
brew install --cask saebyeol
```

### Linux — `.deb` 또는 `.AppImage`

```bash
# Debian / Ubuntu
sudo apt install ./saebyeol_<version>_amd64.deb

# 배포판 무관 — AppImage
chmod +x 새별.AppImage && ./새별.AppImage
```

배포판에 `webkit2gtk-4.1` 이 없으면 Ubuntu 기준 `sudo apt install libwebkit2gtk-4.1-0 libayatana-appindicator3-1`.

### Windows — NSIS Setup 또는 MSI

`saebyeol_<version>_x64-setup.exe` 를 더블클릭. MDM 배포는 `saebyeol_<version>_x64_en-US.msi`.

## 자동 업데이트

이미 설치된 사용자는 메뉴 `새별 → 업데이트 확인…` 또는 다음 실행 시 자동 안내됩니다.

## 직접 다운로드

| OS | 자산 |
|----|------|
| macOS Apple Silicon | `saebyeol_<version>_aarch64.dmg` |
| macOS Intel | `saebyeol_<version>_x64.dmg` |
| Linux x86_64 (deb) | `saebyeol_<version>_amd64.deb` |
| Linux x86_64 (AppImage) | `saebyeol_<version>_amd64.AppImage` |
| Windows x86_64 (NSIS) | `saebyeol_<version>_x64-setup.exe` |
| Windows x86_64 (MSI) | `saebyeol_<version>_x64_en-US.msi` |

> **첫 실행 시 OS 보안 승인 (1회만)**
>
> - **macOS** — Developer ID 서명만 적용된 빌드의 경우 첫 실행 시 *"Apple은 '새별.app' … 악성 코드가 없음을 확인할 수 없습니다"* 경고가 뜰 수 있습니다. **Finder 에서 우클릭 → 열기** 한 번이면 1회 승인 후 정상 실행됩니다 (또는 **시스템 설정 → 개인정보 보호 및 보안 → 그래도 열기**).
> - **Windows** — Authenticode 미서명이라 첫 실행 시 SmartScreen 의 *"PC를 보호했습니다"* 경고가 뜰 수 있습니다. **추가 정보 → 실행** 한 번이면 됩니다.
> - **Linux** — `.deb` 는 apt 가, AppImage 는 사용자가 직접 실행 권한 (`chmod +x`) 을 부여하면 됩니다.
