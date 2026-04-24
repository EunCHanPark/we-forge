# /ping-forge — Register this session with we-forge

세션이 자동 감지되지 않을 때 we-forge에 수동으로 등록합니다.

## 사용법

```
/ping-forge
/ping-forge <label>
```

## 동작

1. `we-forgectl ping <label>` 을 실행해 `~/.we-forge/heartbeats/<pid>.json` 에 heartbeat 파일 기록
2. `we-forgectl sessions` 로 등록 확인
3. Telegram `/status` 에서도 이 세션이 보임

## 세션 감지 방식

| 방법 | 조건 | 설명 |
|------|------|------|
| 자동 (transcript mtime) | 대화 진행 중 | transcript `.jsonl` 파일 수정 시각 기준 |
| 수동 (ping heartbeat) | 세션이 idle하거나 감지 안 될 때 | `/ping-forge` 로 명시적 등록 |

heartbeat는 세션 윈도우(기본 60분) 경과 시 자동 만료됩니다.

$ARGUMENTS
