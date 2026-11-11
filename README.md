# gh-team

팀원들의 GitHub PR 작업/리뷰 현황을 터미널에서 확인하는 인터랙티브 CLI.

## 기능

실행하면 화살표 키로 선택하는 메뉴가 나옵니다.

1. **구성원 설정** — 팀원 GitHub 유저명 추가/삭제
2. **레포 설정** — 조회 대상 레포(`owner/repo`) 추가/삭제
3. **구성원 작업 현황 보기** — 구성원별 오픈 PR 목록과 리뷰 상태(리뷰 대기 / 리뷰중 / 승인됨 / 변경 요청됨 / 초안), 최근 머지된 PR
4. **구성원 리뷰 현황 보기** — 구성원별 리뷰 요청받은 열린 PR 개수와 목록

## 빌드 & 실행

```bash
cargo build --release
./target/release/gh-team
# 또는 개발 중엔
cargo run
```

## 토큰 설정

토큰은 파일에 저장되지 않고 각자 환경변수로 설정합니다.

```bash
export GITHUB_TOKEN=ghp_xxxx
```

[github.com/settings/tokens](https://github.com/settings/tokens)에서 발급.
private 레포를 조회하려면 classic 토큰은 `repo` 스코프,
fine-grained 토큰은 대상 레포의 Pull requests **Read** 권한이 필요합니다.

셸 프로필(`~/.zshrc` 등)에 넣어두면 편합니다.

## 설정 파일

구성원/레포 목록은 아래에 저장됩니다 (토큰은 저장 안 됨):

- macOS: `~/Library/Application Support/gh-team/config.json`
- Linux: `~/.config/gh-team/config.json`

## 참고

- GitHub Search API 사용 (인증 시 분당 30회 제한). 팀 인원이 많고 PR이 많으면 조회가 느려질 수 있습니다.
- 각 검색은 최대 30개(최근 머지는 5개)까지 표시합니다.
