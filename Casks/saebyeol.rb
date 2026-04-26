cask "saebyeol" do
  version "0.1.1"

  arch arm: "aarch64", intel: "x64"

  # arch 별로 dmg 가 따로 빌드되므로 SHA256 도 둘로 분리한다. release CI 의
  # bump-cask-sha 잡이 새 태그마다 두 값을 모두 자동 갱신한다.
  sha256 arm:   "7b7ec866adddd894160def196648b44a57e79c3ee3ff048014386a07f3585280",
         intel: "b9657aaf3c739bab9b316e918d7127d77deb2d482a1bfc8cba0ffa3b2fce4fb2"

  # productName "새별" 의 ASCII-sanitize 결과로 tauri 가 만든 dmg 자산은
  # `_#{version}_#{arch}.dmg` 형태로 prefix 가 비어 있지만, release 워크
  # 플로의 rename 단계가 사용자에게 보이는 이름을 saebyeol_*.dmg 로 다시
  # 올린다. .app 폴더는 새별.app 로 그대로 유지된다.
  url "https://github.com/leaf-kit/saebyeol.md/releases/download/v#{version}/saebyeol_#{version}_#{arch}.dmg"
  name "Saebyeol"
  name "새별"
  desc "Markdown editor with built-in Hangul IME (모아치기 · 세벌식 · 두벌식)"
  homepage "https://leaf-kit.github.io/saebyeol/"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :big_sur"

  app "새별.app"

  # Developer ID Application (TeamID 54B4BWWQ57) 로 정식 서명되어 있다.
  # Apple notary 노터리제이션은 아직 완료되지 않아 첫 실행 시 macOS
  # Gatekeeper 가 "확인되지 않은 개발자" 경고를 띄울 수 있다. 사용자는
  # Finder 에서 새별.app 우클릭 → "열기" 한 번이면 1회 승인 후 정상 실행
  # 된다 (또는 시스템 설정 → 개인정보 보호 및 보안 → "그래도 열기").

  # 앱을 제거할 때 함께 청소할 사용자 데이터 — 설정·자동완성 사용자
  # 사전·학습된 n-gram·캐시.
  zap trash: [
    "~/Library/Application Support/dev.leaf.sbmd",
    "~/Library/Caches/dev.leaf.sbmd",
    "~/Library/Preferences/dev.leaf.sbmd.plist",
    "~/Library/Saved Application State/dev.leaf.sbmd.savedState",
    "~/Library/WebKit/dev.leaf.sbmd",
  ]
end
