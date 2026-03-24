# SC2 Coop Info
- [English](README.md)
- [한국어](README.ko.md)

**스타크래프트 II 협동전**을 위한 Rust/Tauri 기반 데스크톱 오버레이 및 리플레이 분석 도구입니다.

이 저장소는 **FluffyMaguro**가 만들었던 **SC2 Coop Overlay**를 현대적인 기술 스택으로 다시 구현하고 발전시키기 위한 프로젝트입니다. 원작이 제공하던 기능성과 사용 흐름은 최대한 유지하면서, Rust 중심 구조와 Tauri 데스크톱 셸 기반으로 옮기는 것을 목표로 하고 있습니다.

원본 프로젝트:
- https://github.com/FluffyMaguro/SC2_Coop_Overlay

이 저장소의 릴리스 페이지:
- https://github.com/skyser2003/sc2_coop_info/releases

## 원본 프로젝트에 대해

이 프로젝트는 기존 SC2 Coop Overlay가 협동전 커뮤니티에서 오랫동안 유용하게 쓰여 왔기 때문에 시작되었습니다. UI 구성, 사용 흐름, 전반적인 방향성은 원작에서 많은 영향을 받았으며, 이 저장소는 그 경험을 유지한 채 오래된 구현을 Rust 기반으로 재구성하고 개선하는 데 중점을 두고 있습니다.

## 현재 제공하는 기능

- 투명한 인게임 오버레이 창
- 실시간으로 설정을 수정할 수 있는 설정 창
- 리플레이 기록 조회
- 플레이어에 대한 메모 기능
- 주간 돌연변이 정보 추적
- 맵, 사령관, 동맹, 지역, 난이도, 유닛 관련 통계 조회
- 더 자세한 통계를 위한 상세 분석 캐시 생성
- 사령관 랜덤 선택기
- 프로세스 모니터링 기능이 포함된 성능 오버레이
- 오버레이 제어를 위한 전역 단축키
- 시스템 트레이 연동
- 네이티브 폴더 선택기 및 Windows 시작 프로그램 등록 지원
- Rust 기반 리플레이 파싱 및 분석
- 영어/한국어 지원

## 아키텍처

현재 앱은 `tauri-overlay` 데스크톱 애플리케이션과 여러 Rust crate들을 중심으로 구성되어 있습니다.

- `tauri-overlay`
  - Tauri 데스크톱 셸
  - React + Vite 프론트엔드
  - Rust 백엔드 명령 처리 및 창 관리
- `s2coop-analyzer`
  - 리플레이 및 통계 분석 로직
  - 캐시 생성
- `s2protocol-port`
  - SC2 리플레이 프로토콜 파싱 지원

## 주요 기능

### 오버레이

- 게임 종료 후 리플레이 요약 정보 표시
- 표시/숨김 및 리플레이 탐색용 단축키 지원
- 게임 시작 시 플레이어 정보 표시 지원
- 차트 표시 여부 및 색상 사용자 지정 지원

### 설정 앱

현재 설정 창에는 다음 탭이 포함되어 있습니다.

- `Settings`
- `Games`
- `Players`
- `Weeklies`
- `Statistics`
- `Randomizer`
- `Performance`
- `Links`

### 리플레이 분석

- 스타크래프트 II 계정 폴더에서 리플레이 데이터 읽기
- 리플레이 목록 및 요약 테이블 생성
- 플레이어, 사령관, 맵, 난이도, 지역 정보 추적
- 단순 분석 및 상세 분석 지원
- 더 풍부한 통계를 위한 상세 분석 캐시 출력 저장
- 리플레이 채팅 보기 및 파일 위치 열기 기능 제공

### 성능 오버레이

- 별도의 투명한 성능 창
- 선택한 프로세스 추적
- 전용 단축키 및 저장된 창 위치/크기 지원

## 스크린샷

**설정 창**

![스크린샷](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image1.ko.png)

**리플레이 목록**

![스크린샷](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image2.ko.png)

**플레이어 목록**

![스크린샷](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image3.ko.png)

**주간 돌연변이 목록**

![스크린샷](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image4.ko.png)

**각종 통계**

![스크린샷](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image5.ko.png)

**사령관 랜덤 선택기**

![스크린샷](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image6.ko.png)

**성능 오버레이**

![스크린샷](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image7.ko.png)

## 개발 환경에서 실행하기

### 사전 요구 사항

- Rust 툴체인
- Node.js 및 npm
- Windows 환경 권장

### 프론트엔드 + Tauri 개발 실행

```powershell
cd tauri-overlay
npm install
npm run tauri dev # or cargo tauri dev
```

## 빌드

```powershell
cd tauri-overlay
npm install
cargo tauri build
```

## 설정 및 사용 시 참고 사항

- 리플레이를 분석하려면 스타크래프트 II 계정 폴더에 접근할 수 있어야 합니다.
- 설정 창의 많은 옵션은 실행 중인 오버레이 백엔드에 실시간으로 반영됩니다.
- `settings.json`은 사용자가 명시적으로 저장했을 때 업데이트됩니다.
- 인게임 오버레이를 제대로 사용하려면 스타크래프트 II를 창 모드/전체 창 모드로 실행해야 합니다.

## Windows 관련 참고 사항

- Windows가 주요 지원 플랫폼입니다.
- 이 앱은 Windows 환경에 맞춘 트레이 동작, 전역 단축키, 시작 프로그램 등록, 오버레이 창 배치 로직을 포함하고 있습니다.

## 개발 스택

- 프론트엔드: React, Vite, Material UI, Tauri API
- 백엔드: Rust, Tauri

## 저장소 상태

이 저장소는 기존 SC2 Coop Overlay의 기능을 Rust/Tauri 기반으로 옮기는 작업이 진행 중인 프로젝트입니다. 일부 동작은 원본 프로젝트와의 일관성을 유지하도록 설계되어 있으며, 일부 오래된 사항은 제거되거나 새롭게 작성되고 있습니다.

## 피드백

버그 제보, 피드백, 제안은 이슈를 남겨주시거나 아래 주소로 보내주시면 됩니다.

- mailto:sc2coopinfo@gmail.com
