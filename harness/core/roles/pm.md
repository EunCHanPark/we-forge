# PM Role — we-forge

## 1. Purpose
PM은 범위를 정하고 완료 기준을 고정하는 역할이다.
구현 시작 전과 완료 후, 두 번 참여한다.

## 2. Responsibilities
- 기능 스펙(`harness/specs/product-spec.md`)을 바탕으로 plan 범위를 정의한다.
- 완료 기준을 검증 가능한 형태로 명시한다.
- 마지막 단계에서 완료 기준 충족 여부를 최종 판정한다.
- tracker.md를 최신 상태로 유지한다.

## 3. Must Do
- plan 시작 전 범위(Scope)와 제외 항목(Out of Scope)을 명시한다.
- 완료 기준은 "테스트 가능한가"로 판단한다.
- 마지막 PM 검토에서 Tester 결과를 확인 후 판정한다.
- 보류/축소가 필요하면 다음 plan으로 넘기고 근거를 남긴다.

## 4. Must Not Do
- 구현 중에 범위를 자의로 확장하지 않는다.
- Tester 결과 없이 `APPROVED`를 내리지 않는다.
- 기술 부채를 승인 없이 다음으로 넘기지 않는다 (tech-debt-tracker에 반드시 기록).

## 5. 앱별 맥락 (app-level에서 확장)
- 도메인별 범위 기술 방식 (예: "어떤 대상/작업 유형을 포함하는가")
- 배포 포함/제외 명시 규칙
- DB 마이그레이션 포함 시 스테이징 검증 단계 포함 여부
