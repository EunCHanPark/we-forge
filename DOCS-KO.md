# we-forge — 한글 사용 설명서

> Claude Code CLI를 위한 **백그라운드 패턴 학습 + 멀티-LLM 위임 확장**
>
> 터미널에서 반복하는 작업을 자동으로 감지해서 재사용 가능한 skill로 만들어주고,
> 필요할 때 Codex·Gemini 같은 다른 LLM을 Claude Code 안에서 바로 호출하게 해줍니다.

GitHub: https://github.com/EunCHanPark/we-forge

---

## 1. we-forge가 무엇인가

Claude Code CLI 위에 얹는 **작은 확장 레이어**입니다. 독립 제품이 아니라 `~/.claude/` 디렉토리 안에 파일 몇 개를 추가해서 Claude Code의 **숨은 기능처럼** 동작하게 만든 도구입니다.

핵심 기능 2가지:

1. **반복 작업 자동 학습** — 터미널에서 3번 이상 반복되는 명령 패턴을 감지해서 `SKILL.md` 파일로 자동 합성. 다음번에 Claude가 같은 상황을 만나면 학습된 skill이 자동 로드됨.
2. **타 LLM 위임** — `/ask-codex`, `/ask-gemini` 슬래시 명령으로 OpenAI Codex CLI 또는 Google Gemini CLI를 Claude Code 안에서 one-shot 호출. Claude Code는 메인 오케스트레이터로 유지, 다른 LLM은 서브 툴.

### 무엇이 아닌가 (중요)

다음과는 **다릅니다**:

| 오해 | 실제 |
|---|---|
| "Hermes Agent나 OpenClaw 같은 독립 AI 에이전트 제품" | ❌ 아님. Claude Code가 없으면 작동 안 함. 자체 CLI 없음 |
| "Claude Code를 대체하거나 감싸는 wrapper" | ❌ 아님. Claude Code를 **확장**만 함. 사용자 경험 변화 없음 |
| "클라우드 서비스" | ❌ 아님. 모든 데이터·처리가 로컬 머신에 머무름 (단, `/ask-codex`·`/ask-gemini` 호출 시 해당 CLI가 외부 API 호출) |
| "별도로 매일 실행해야 하는 툴" | ❌ 아님. 설치 후 **완전 자동**. 사용자가 명시적으로 호출할 필요 없음 |

---

## 2. 어떻게 동작하는가

### 전체 흐름도

```
  ① 세션 종료 시 이벤트 수집                ② 시간당 정규화 + 패턴 감지
┌───────────────────────┐                  ┌──────────────────────────┐
│ Stop hook             │                  │ launchd (macOS)          │
│  stop-telemetry.sh    │                  │ 또는 cron (Linux/WSL)    │
│    ├─ bash_history    │     append        │                          │
│    ├─ transcript      │────events.jsonl──▶│  tick.sh                 │
│    └─ redact filter   │                  │   ├─ 회전(50MB 초과 시)  │
└───────────────────────┘                  │   ├─ normalize.py        │
                                           │   │    ├─ 시크릿 필터    │
                                           │   │    ├─ 정규화          │
                                           │   │    └─ 3회+ 3세션 감지 │
                                           │   └─ queue 비어 있음? exit│
                                           └────────────┬─────────────┘
                                                        │ 큐 있음
                                                        ▼
                                    ③ we-forge agent가 memory 보고 판단
                                    ┌─────────────────────────────────┐
                                    │ claude --agent we-forge -p tick │
                                    │   memory: user (누적 학습)       │
                                    │   ↓                              │
                                    │ pattern-detector    (read-only)  │
                                    │   ↓                              │
                                    │ skill-synthesizer   (pending/)   │
                                    │   ↓                              │
                                    │ quality-auditor     (PASS/       │
                                    │                      REVISE/    │
                                    │                      REJECT)   │
                                    └───────────────┬─────────────────┘
                                                    ▼
                             ~/.claude/skills/learned/<skill-name>/SKILL.md
```

### 단계별 상세

#### 단계 ① — 이벤트 수집 (실시간)

**언제**: Claude Code 세션이 종료될 때마다 (Stop hook)
**어디서**: `~/.claude/hooks/stop-telemetry.sh`
**무엇을 수집**:
- `~/.bash_history` 또는 `~/.zsh_history` 의 신규 라인 (마지막 offset 이후)
- Claude Code transcript 파일 (`~/.claude/projects/*/*.jsonl`) 의 tool_use 이벤트
**필터**:
- `redact.sh` 가 API 키, 토큰, 비밀번호 패턴 감지 시 **라인 전체 drop** (마스킹 아님)
- Shannon entropy ≥ 4.0 인 32자+ 고엔트로피 토큰도 drop

**결과**: `~/.claude/learning/data/events.jsonl` 에 append-only로 누적.

```json
{"ts":"2026-04-23T12:00:00Z","session_id":"sess-abc","source":"bash","raw":"git status","normalized":null}
```

#### 단계 ② — 정규화 + 패턴 감지 (매시간)

**언제**: 매 시간 정각 (macOS launchd, Linux/WSL cron)
**어디서**: `~/.claude/learning/tick.sh` + `normalize.py`
**동작**:
1. events.jsonl 읽어서 각 이벤트 canonical 형태로 변환:
   - `git status` → `git status` (불변)
   - `ls -la /Users/foo/project` → `ls -la <PATH>`
   - `docker run myimage:1.2.3` → `docker run <STR>`
2. 정규화된 pattern 별로 집계
3. **승격 조건 3가지 모두 만족** 시 promotion queue 에 추가:
   - 해당 pattern 이 **3회 이상** 등장
   - 최소 **3개의 distinct session_id** 에서 등장 (우연적 1회성 세션 방지)
   - `~/.claude/skills/learned/` 에 이미 같은 slug 없음
   - `rejected.txt` poison list 에 없음

**큐 비어 있으면 토큰 0원 종료** (이것이 핵심 비용 설계).

#### 단계 ③ — we-forge agent 가 orchestration (queue 있을 때만)

**언제**: queue 에 엔트리 있을 때만
**어디서**: `claude --agent we-forge -p "tick"` (헤드리스 Claude Code 세션)
**동작**:
1. `~/.claude/agent-memory/we-forge/MEMORY.md` 에서 과거 판정 이력 읽기
2. 메모리 blocklist 에 있는 slug 는 건너뛰기
3. `pattern-detector` sub-agent 호출 → 큐를 distinct candidate list 로 축소
4. `CLAUDE_TICK_MAX_CANDIDATES=5` 상한 적용 (초과분은 다음 tick 으로)
5. 각 candidate 에 대해:
   - `skill-synthesizer` sub-agent → `pending/<slug>/SKILL.md` 초안 작성
   - `quality-auditor` sub-agent → 6개 rubric 검사 후 판정
6. 판정 결과 메모리에 기록 (다음 tick 에서 활용)
7. queue 엔트리 제거 또는 revise_count += 1

#### 단계 ④ — 6-point rubric 품질 감사

`quality-auditor` 가 **모두 통과해야 PASS** 하는 기준:

1. **Frontmatter 유효** — YAML 파싱 OK, `name`, `description`, kebab-case slug 일치
2. **본문 구조** — `## When to use`, `## Steps`, `## Example` 3섹션 있음, Steps ≥ 2
3. **시크릿 없음** — 본문 각 라인이 `redact.sh --check` 통과
4. **중복 아님** — 기존 learned skill 과 slug/description 중첩 없음
5. **Genuine pattern** — distinct `session_id` 3개 이상 (cron-only 수집 방지)
6. **의심 동작 없음 (프롬프트 인젝션 방어)** — 본문에 다음 패턴 없어야:
   - 외부 URL (localhost 제외)
   - `sudo`, `su -`
   - `curl | sh`, `eval`, `base64 -d | sh`
   - 범위 밖 `rm -rf`
   - `.env`, `.aws/`, `.ssh/`, `id_rsa` 경로 언급

**하나라도 실패 → REVISE** (2회까지), 3번째 실패 → REJECT + `rejected.txt` 영구 블록.
단 **rubric 3·6번 실패 시 즉시 REJECT** (revise 없이, 보안 관련).

---

## 3. 어떤 효과가 있는가

### 효과 1 — 반복 작업 자동 스킬화

예시 시나리오:
- 사용자가 여러 세션에서 `git status` → `git add .` → `git commit -m "..."` 순서를 **반복**
- we-forge 가 3회+ 감지 → `quick-commit` 같은 skill 자동 생성
- 다음에 Claude 가 커밋 관련 작업할 때 이 skill 이 context 에 로드 → **일관된 워크플로우**
- 사용자가 설정 파일 안 만지고도 자기 습관에 맞게 Claude 커스터마이즈 되어감

### 효과 2 — 멀티-LLM 체인 안에서 Claude Code 중심 유지

예시:
```
사용자: "이 500K 라인 로그 분석해서 에러 패턴 뽑아줘"
Claude Code (you): "긴 컨텍스트는 Gemini 가 적합합니다. /ask-gemini 호출합니다"
→ /ask-gemini 명령 내부에서 gemini CLI 실행 → 결과 반환
→ Claude 가 결과 해석해서 사용자에게 요약 제공
```

Claude Code 는 메인 오케스트레이터로 유지되며, 다른 LLM 은 one-shot 서브태스크만 수행. **cross-LLM 라우팅을 Claude Code 안에서 손쉽게**.

### 효과 3 — 제로-스펜드 설계로 비용 안전

- tick.sh 매시간 실행되지만 **bash 만 사용** → API 토큰 0
- queue 비어있으면 `claude --agent we-forge` 자체가 호출 안 됨 → 완전 무비용
- queue 있을 때만 we-forge 기동 → 실제로 학습할 것이 있을 때만 과금
- `CLAUDE_TICK_MAX_CANDIDATES=5` 상한으로 최악의 경우 시간당 5 candidate 만 처리

### 효과 4 — 프라이버시 & 보안

- 모든 데이터 **로컬 디스크**에 머무름. 외부 전송 없음 (단, Claude 에이전트 호출 시 Anthropic API 통신)
- 시크릿 드롭 이중화: stop-telemetry 수집 시 1차 필터 + normalize.py 집계 시 2차 체크
- 의심 동작 패턴 rubric 으로 프롬프트 인젝션 방어
- 헤드리스 tick subprocess 안에서만 ECC hook 일시 비활성화 (인터랙티브 세션엔 영향 없음)
- `redact.sh --self-test` 로 13개 시나리오 검증 가능

### 효과 5 — Cross-run 메모리 누적

we-forge agent 에는 `memory: user` 가 설정되어 있어서:
- 과거에 REJECT 한 패턴을 기억해서 재합성 시도 안 함 (비용 절약)
- 사용자 선호 포맷이나 특이 케이스를 MEMORY.md 에 축적
- 매 10번째 tick 마다 "쓰이지 않는 skill" 감지해서 deprecation 후보 제시

---

## 4. 설치

### 4-1. 사전 준비

- Claude Code CLI 설치 (https://docs.claude.com)
- `jq`, `python3`, `bash`
- git

### 4-2. macOS

```bash
# 저장소 clone
git clone https://github.com/EunCHanPark/we-forge.git
cd we-forge

# 자체 테스트 먼저
./install.sh --test

# 설치 (~/.claude/ 에 파일 복사 + settings.json Stop hook 병합 + 백업)
./install.sh

# install.sh 가 출력하는 "macOS scheduler" 블록의 명령을 그대로 실행:
mkdir -p ~/Library/LaunchAgents
sed -e "s|__USER__|$USER|g" \
    -e "s|__HOME__|$HOME|g" \
    -e "s|__CLAUDE_HOME__|$HOME/.claude|g" \
    "$(pwd)/launchd/com.we-forge-tick.plist.template" \
    > "$HOME/Library/LaunchAgents/com.$USER.we-forge-tick.plist"
launchctl load -w "$HOME/Library/LaunchAgents/com.$USER.we-forge-tick.plist"

# 즉시 한 번 수동 실행 (선택)
launchctl start com.$USER.we-forge-tick
tail -n 10 ~/.claude/learning/data/tick.log
```

### 4-3. Linux

```bash
git clone https://github.com/EunCHanPark/we-forge.git
cd we-forge
./install.sh --test
./install.sh

crontab -e
# 붙여넣기:
0 * * * * /bin/bash -lc '~/.claude/learning/tick.sh >> ~/.claude/learning/data/tick.log 2>&1'
```

### 4-4. Windows Server (WSL2)

별도 가이드: [`WSL-SETUP.md`](WSL-SETUP.md) 참고. 요약:
- Windows Server 에 WSL2 + Ubuntu 설치
- `/etc/wsl.conf` 에 `[boot] systemd=true` 추가
- WSL 내부에서 cron 활성화 (`sudo systemctl enable --now cron`)
- 위 Linux 절차 그대로 진행

---

## 5. 사용법

### 5-1. 자동 동작 (기본)

설치 후 **아무 것도 안 해도 됩니다**. 다음이 자동으로 일어남:
- Claude Code 세션 종료 시마다 이벤트 수집
- 매 시간 정각 tick 실행
- 3회+ 반복 패턴 자동 합성
- PASS 판정된 skill 이 `~/.claude/skills/learned/` 에 등록
- 이후 Claude Code 세션이 자동 로드

### 5-2. 모니터링

현재 상태 확인:

```bash
# 수집된 이벤트 총 개수
wc -l ~/.claude/learning/data/events.jsonl

# 학습 대기 중인 패턴
cat ~/.claude/learning/data/promotion_queue.jsonl

# 등록된 skill 목록
ls ~/.claude/skills/learned/

# 최근 tick 로그
tail -n 20 ~/.claude/learning/data/tick.log

# 감사 판정 이력 (PASS/REVISE/REJECT)
tail -n 20 ~/.claude/learning/data/ledger.jsonl
```

Claude Code 세션 안에서 자연어로:
```
/skill-report
```
→ Telemetry + 대기 패턴 + 학습된 skill + 최근 판정 종합 리포트.

### 5-2a. 세션 감지 및 수동 등록

we-forge는 활성 Claude Code 세션을 자동 감지하지만, 세션이 idle 상태일 때 
명시적으로 등록할 수도 있습니다.

**자동 감지** — transcript 파일 수정 시각 기준:
```bash
# 지난 60분 내 활성 세션 조회
we-forgectl sessions

# 지난 120분 내 활성 세션 조회
we-forgectl sessions --window 120
```

**수동 등록** — heartbeat ping 방식 (세션 idle 시):
```bash
# 현재 세션을 we-forge에 등록
! we-forgectl ping

# 레이블과 함께 등록 (선택사항)
! we-forgectl ping my-feature-branch
```

Claude Code 세션 안에서:
```
/ping-forge
/ping-forge my-feature-branch
```

Heartbeat 파일은 자동으로 60분 후 만료되며, `we-forgectl status`에서도 세션 목록을 확인할 수 있습니다.

### 5-3. 타 LLM 위임 (`/ask-codex`, `/ask-gemini`)

Claude Code 세션 안에서:
```
/ask-codex 이 파이썬 함수를 더 Pythonic 하게 refactor 해줘
```

또는 긴 컨텍스트가 필요할 때:
```
/ask-gemini 이 500K 라인 log 에서 OOM 관련 패턴만 뽑아줘
```

내부 흐름:
1. `codex` 또는 `gemini` CLI 가 PATH 에 있는지 확인
2. 사용자 질문에 시크릿 없는지 정규식 검사 (API 키, 토큰 등)
3. 해당 CLI 를 Bash tool 로 호출
4. stdout 을 `**Codex says:**` 또는 `**Gemini says:**` 헤더와 함께 반환

**주의**: 이 명령은 Claude 가 해석·재작성하지 않고 **verbatim 반환**합니다.

### 5-4. 학습 파이프라인 수동 실행

평소엔 cron 이 알아서 하지만, 테스트하거나 바로 처리하고 싶을 때:

**macOS**:
```bash
launchctl start com.yukibana.we-forge-tick
tail -f ~/.claude/learning/data/tick.log
```

**Linux/WSL**:
```bash
~/.claude/learning/tick.sh
```

또는 Claude Code 인터랙티브 세션 안에서:
```
/watch-and-learn
```

### 5-5. we-forge agent 직접 호출

헤드리스로 한 번 돌려보기 (memory 를 활용하는 전체 파이프라인):
```bash
claude --agent we-forge -p "tick"
```

또는 인터랙티브 세션으로 we-forge 로 들어가기 (대화형 디버깅):
```bash
claude --agent we-forge
```
→ 기본 Claude Code prompt 대신 we-forge system prompt 로 시작. 학습 파이프라인 디버깅할 때 유용.

---

## 6. 파일 구조

### 저장소

```
we-forge/
├── README.md                         # 영문 기술 레퍼런스
├── DOCS-KO.md                        # 이 파일 (한글 사용법)
├── WSL-SETUP.md                      # Windows Server → WSL2 설치
├── install.sh                        # OS-aware 설치 스크립트
├── crontab.example                   # cron 엔트리 (Linux/WSL)
├── agents/
│   ├── monitor-sentinel.md           # 읽기 전용 텔레메트리 요약
│   ├── pattern-detector.md           # 큐 중복 제거 + 클러스터링
│   ├── skill-synthesizer.md          # SKILL.md 초안 작성
│   ├── quality-auditor.md            # PASS/REVISE/REJECT 감사
│   └── we-forge.md                   # 메인 세션 오케스트레이터 (memory 有)
├── commands/
│   ├── watch-and-learn.md            # 인터랙티브 파이프라인 트리거
│   ├── skill-report.md               # 읽기 전용 상태 리포트
│   ├── ask-codex.md                  # Codex CLI 위임
│   └── ask-gemini.md                 # Gemini CLI 위임
├── hooks/
│   └── stop-telemetry.sh             # 세션 종료 시 이벤트 수집
├── learning/
│   ├── tick.sh                       # cron/launchd 진입점
│   ├── redact.sh                     # 시크릿 필터 (self-test 포함)
│   ├── normalize.py                  # 정규화 + 승격 규칙
│   └── settings.snippet.json         # Stop hook 병합 템플릿
└── launchd/
    └── com.we-forge-tick.plist.template   # macOS LaunchAgent 템플릿
```

### 설치 후 `~/.claude/`

```
~/.claude/
├── settings.json                     # Stop hook 엔트리 추가됨 (백업됨)
├── settings.json.bak.<ISO>           # 설치 시점 백업
├── agents/
│   ├── we-forge.md
│   ├── monitor-sentinel.md
│   ├── pattern-detector.md
│   ├── skill-synthesizer.md
│   └── quality-auditor.md
├── commands/
│   ├── watch-and-learn.md
│   ├── skill-report.md
│   ├── ask-codex.md
│   └── ask-gemini.md
├── hooks/
│   └── stop-telemetry.sh
├── learning/
│   ├── tick.sh
│   ├── redact.sh
│   ├── normalize.py
│   └── data/
│       ├── events.jsonl              # 수집된 이벤트 (append-only)
│       ├── patterns.jsonl            # 정규화된 패턴 빈도 테이블
│       ├── promotion_queue.jsonl     # 학습 대기 큐
│       ├── ledger.jsonl              # 감사 결정 기록
│       ├── rejected.txt              # poison list
│       ├── state.json                # 커서 (bash/transcript offset)
│       ├── tick.log                  # tick 진단 로그
│       └── telemetry.log             # Stop hook 진단 로그
├── agent-memory/
│   └── we-forge/
│       └── MEMORY.md                 # we-forge 의 cross-run 학습
└── skills/
    └── learned/
        ├── <skill-name-1>/SKILL.md   # 학습 완료된 skill
        ├── <skill-name-2>/SKILL.md
        └── pending/                   # synthesizer 초안 대기
            └── <slug>/
                ├── SKILL.md
                └── meta.json
```

---

## 7. 트러블슈팅

### 7-1. 이벤트가 안 쌓임

```bash
wc -l ~/.claude/learning/data/events.jsonl
# 0 이 나오면 Stop hook 이 안 돌고 있는 것
```

확인:
```bash
jq '.hooks.Stop' ~/.claude/settings.json
# stop-telemetry.sh 가 리스트에 있어야 함

ls -la ~/.claude/hooks/stop-telemetry.sh
# 실행 권한 있어야 함 (-rwx...)

# Stop hook smoke test
echo '{"session_id":"t","transcript_path":"/dev/null","stop_hook_active":false,"cwd":"/tmp"}' \
  | ~/.claude/hooks/stop-telemetry.sh
echo "exit=$?"
```

exit code 0 이 아니거나 telemetry.log 에 에러 있으면 그쪽부터 수정.

### 7-2. tick 이 안 돌음 (macOS)

```bash
launchctl list | grep we-forge
# LastExitStatus 숫자가 0 이어야 정상

tail -n 50 ~/.claude/learning/data/tick.log
# 최근 :00 에 "tick begin" 라인 있어야 함

# 수동 트리거로 즉시 확인
launchctl start com.$USER.we-forge-tick
sleep 3
tail -n 10 ~/.claude/learning/data/tick.log
```

아무 로그도 없으면:
- plist 가 로드 안 됐을 수 있음: `launchctl load -w ~/Library/LaunchAgents/com.$USER.we-forge-tick.plist`
- 절전 모드에서 :00 을 놓쳤을 수 있음 (macOS 는 sleep 중 launchd 도 멈춤)

### 7-3. macOS cron 실패 "Operation not permitted"

macOS 는 보통 cron 대신 launchd 쓰세요. 우리 설치 가이드도 macOS 는 launchd 기본.
굳이 cron 쓰려면 **Terminal 앱과 `/usr/sbin/cron` 둘 다** System Settings → Privacy & Security → Full Disk Access 에 추가하고 Terminal 완전 재시작 필요.

### 7-4. WSL 에서 tick 이 돌지만 이벤트 수집이 0

```bash
# WSL 은 zsh 대신 bash 기본. bash_history 확인:
ls -la ~/.bash_history
# 있어야 함. 없으면 최근 bash 명령 실행 후 재확인.
```

### 7-5. ECC gateguard 때문에 헤드리스 tick stall

`learning/tick.sh` 가 `claude --agent we-forge` 호출 시 subshell 에 다음 env 를 설정합니다:
```bash
ECC_DISABLED_HOOKS=pre:bash:gateguard-fact-force,pre:edit-write:gateguard-fact-force,pre:observe:continuous-learning
```
이 설정으로 ECC gateguard 가 헤드리스에서는 비활성화됩니다. 인터랙티브 세션에는 영향 없음.

만약 여전히 stall 나면:
```bash
tail -n 100 ~/.claude/learning/data/tick.log
```
확인하고 ECC 기타 hook ID 가 차단하는지 파악.

### 7-6. 이상한 skill 이 learned/ 에 생김

```bash
ls ~/.claude/skills/learned/
# 의심스러운 디렉토리 있으면:
cat ~/.claude/skills/learned/<slug>/SKILL.md

# 삭제:
rm -rf ~/.claude/skills/learned/<slug>

# 재 promotion 방지 (poison list 에 추가):
echo "<canonical-pattern>" >> ~/.claude/learning/data/rejected.txt
```

rubric #6 이 차단 못한 케이스면 `agents/quality-auditor.md` 의 의심 패턴 리스트에 추가하는 PR 환영.

---

## 8. 제거 (Uninstall)

설치는 idempotent 지만 uninstall 스크립트는 제공하지 않습니다. 수동:

```bash
# 1. launchd / cron 중지
# macOS:
launchctl unload ~/Library/LaunchAgents/com.$USER.we-forge-tick.plist
rm ~/Library/LaunchAgents/com.$USER.we-forge-tick.plist
# Linux/WSL:
crontab -e   # tick.sh 라인 삭제

# 2. settings.json 에서 Stop hook 제거
# 가장 최근 백업 복원:
ls ~/.claude/settings.json.bak.*
cp ~/.claude/settings.json.bak.<최근-타임스탬프> ~/.claude/settings.json

# 또는 jq 로 정확히 우리 것만 제거:
jq '.hooks.Stop |= map(.hooks |= map(select(.command != "~/.claude/hooks/stop-telemetry.sh")))' \
   ~/.claude/settings.json > /tmp/s && mv /tmp/s ~/.claude/settings.json

# 3. 파일 삭제
rm -rf ~/.claude/hooks/stop-telemetry.sh
rm -rf ~/.claude/learning/
rm -rf ~/.claude/agent-memory/we-forge/
rm ~/.claude/agents/{monitor-sentinel,pattern-detector,skill-synthesizer,quality-auditor,we-forge}.md
rm ~/.claude/commands/{watch-and-learn,skill-report,ask-codex,ask-gemini}.md

# 4. 학습된 skill 은 유지 vs 삭제 선택:
# 유지:   그대로 두면 Claude 가 계속 사용
# 삭제:   rm -rf ~/.claude/skills/learned/
```

---

## 9. FAQ

**Q. 이미 내 harness 에 continuous-learning-v2 (ECC skill) 가 있는데 중복 아닌가?**
A. 기능 일부가 겹치지만 **저장소 완전 분리**. ECC 의 instinct 시스템은 `~/.claude/homunculus/` 에, we-forge 학습 결과는 `~/.claude/skills/learned/` 에 기록. 파일 충돌 없음. 리소스 중복(같은 이벤트 2번 관찰)만 있고, 우리는 `ECC_DISABLED_HOOKS` 로 헤드리스에서는 ECC observe 끕니다.

**Q. Claude Pro/Max 구독자만 쓸 수 있나?**
A. Claude Code CLI 사용할 수 있는 모든 플랜에서 작동. 추가 비용은 `claude --agent we-forge` 호출 시마다의 토큰 사용분만. 큐 비면 0원.

**Q. 팀원들과 공유 가능?**
A. 코드는 공개 MIT 이지만 **데이터는 머신별로 분리**. Mac 의 학습 결과가 팀원 Windows 서버로 안 넘어감. 공유 학습 원하면 `events.jsonl` 을 S3/syncthing 으로 동기화해야 하는데 이건 별도 설계 필요.

**Q. 프라이버시 — transcript 수집이 찜찜함.**
A. 이해. 옵션:
- transcript 수집을 아예 끄기: `stop-telemetry.sh` 의 transcript 블록 주석 처리 후 재설치
- raw 필드만 버리기: `normalize.py` 에서 append 시 `raw` 제거 후 `normalized` 만 보존 (정보 보존성과 trade-off)
- 전체 비활성화: 위 "제거" 절차 따르기

**Q. 더 빠르게 학습되게 (예: 2회 반복만 돼도) 할 수 있나?**
A. `learning/normalize.py` 의 승격 조건 수정:
```python
if p["count"] < 3:              # 이 숫자를 2로
    continue
if len(p["sample_session_ids"]) < 3:   # 이것도 2 또는 1로
    continue
```
단 노이즈 많아져서 REJECT 비율 상승 예상. 경험적으로 3/3 이 균형점.

**Q. 비용 상한 조정?**
A. `tick.sh` 에 `CLAUDE_TICK_MAX_CANDIDATES=<N>` 환경변수 추가:
```bash
export CLAUDE_TICK_MAX_CANDIDATES=10   # 기본 5
```
`watch-and-learn.md` 와 `we-forge.md` 가 이 값 honor 함.

**Q. 다른 에이전트 (Hermes/OpenClaw) 와 같이 쓸 수 있나?**
A. 파일 충돌 없음 (저장 경로 다름). 다만 두 시스템이 동시에 cron 돌리면 리소스 중복. 권장: Mac = we-forge 전용, Windows 서버 = Hermes 전용 같은 식으로 분리.

---

## 10. 라이선스 & 기여

- 라이선스: 저장소 참조
- Issue/PR: https://github.com/EunCHanPark/we-forge/issues
- 연관 프로젝트: Everything Claude Code (ECC) 플러그인 — we-forge 는 ECC 와 공존하도록 설계됨

## 11. 변경 이력

`git log --oneline` 으로 전체 이력 확인. 주요 마일스톤:
- `5111235` — 초기 구현 (4 sub-agents + 2 commands)
- `89bf3e2` — 보안 강화 (rubric #6 의심 동작 REJECT + candidate 상한)
- `ad508c2` — events.jsonl race 수정 + revise_count 전파 수정
- `843ff7d` — zsh_history 자동 감지
- `3e5a33d` — `we-forge` 메인 agent 추가 (persistent memory)
- `fdf61ba` — OS 별 설치 가이드 + WSL2 문서 + LaunchAgent 템플릿
- `0146154` — `/ask-codex`, `/ask-gemini` 추가

---

궁금한 점, 문제, 개선 아이디어는 GitHub issue 로. 사용 중 skill 이 이상하게 등록되면 그 컨텐츠 공유해 주시면 rubric 개선에 반영하겠습니다.
