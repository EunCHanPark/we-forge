# Repository Architecture

## 1. 모노레포 구조
- 공통 규칙: `harness/core/`
- 앱별 코드와 하네스: `apps/<app-name>/{src,harness}/`
- 공통 스크립트: `harness/scripts/`

## 2. 의존성 방향
- 서비스 호출은 단방향으로만 흐른다 (Non-Negotiable 1)
- 순환 참조 금지. 새 서비스 추가 시 의존성 방향을 문서에 명시한다.

## 3. 인프라 as Code
- 인프라, 스키마, 설정은 레포에 존재한다 (Non-Negotiable 2)
- 배포 매니페스트 변경은 레포에 반영한 뒤 적용한다.
- secrets/는 PLACEHOLDER만 커밋 (Non-Negotiable 3)
- 로컬 개발과 프로덕션 배포 경로를 분명히 구분한다.

## 4. 배포 원칙
- 의존 대상을 먼저 배포한다. 의존성 역순으로 중단한다.
- 앱별 구체적 배포 순서는 app-level 문서에 명시한다.

## 5. 새 앱/서비스 추가 규칙
- `apps/<app-name>/{src,harness}/` 구조를 따른다.
- app-level harness에 최소 index.md, tracker.md를 둔다.
- 의존성 방향을 app-level architecture 문서에 명시한다.
