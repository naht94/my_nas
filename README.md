# my_nas

Crew(조직) 기반 권한 모델로 파일을 관리하는 소규모 팀단위 NAS 서버입니다.
Web UI(`SvelteKit`)와 API/WebDAV(`Rust + Axum`)를 제공하며, Linux 환경에서 `my_nas + proxy` 조합으로 운영합니다.

## 🛠 기술 스택

- **Back-End**: Rust, Axum, SQLx, Tokio
- **Front-End**: SvelteKit, Vite, Tailwind CSS
- **Database**: SQLite
- **Proxy/Network**: Pingora (HTTPS + HTTP/3), WebDAV (`dav-server`)
- **Infra/Tool**: Linux, Bash, Cargo, npm

## 프로젝트 용도

- Crew 계층(부모/자식) 기반의 접근 제어
- 웹 파일 탐색/업로드/삭제/휴지통 관리
- WebDAV 마운트를 통한 외부 클라이언트 연동
- 로그인/세션/앱 비밀번호 관리
- 주요 관리 이벤트 감사 로그(Audit Log) 조회

## 폴더 시스템 구현 요약

- 논리 구조는 `folders`/`files` 테이블로 관리합니다.
- Crew 루트 폴더를 기준으로 하위 폴더/파일 트리를 구성합니다.
- 물리 파일은 `STORAGE_PATH` 하위에 파일 ID(UUID) 기반 샤딩 경로로 저장합니다.
  - 예: `STORAGE_PATH/ab/cd/<file_id>`
- 파일명/경로는 DB 메타데이터로 관리하며, 실제 디스크 경로는 내부 최적화(샤딩) 용도입니다.
- 읽기/쓰기 권한은 Crew 멤버십 정책을 통해 검증합니다.

## NAS로 사용하기

### 1) 요구 사항

- Linux 서버/미니PC
- Rust/Cargo, Node.js/npm
- TLS 인증서/개인키 파일 (`CERT_PATH`, `KEY_PATH`)

### 2) 환경 변수 준비

프로젝트 루트에 `.env`를 만들고 최소 값을 설정합니다.

```env
DATABASE_URL="sqlite://db/nas.db"
STORAGE_PATH="/path/to/storage"
CERT_PATH="/path/to/fullchain.pem"
KEY_PATH="/path/to/privkey.pem"
```

### 3) 실행

```bash
# 프로덕션(권장): my_nas + proxy
./scripts/nas.sh start --build

# 개발 모드(vite 포함)
./scripts/nas.sh start --build --dev
```

주요 명령어:

```bash
./scripts/nas.sh status
./scripts/nas.sh logs
./scripts/nas.sh restart --build
./scripts/nas.sh stop
```

### 4) 접속

- Web NAS: `https://<host>:48483/NAS`
- WebDAV: `https://<host>:48483/webdav` (또는 `:48484/webdav`)
  - WebDAV는 앱 비밀번호(Basic Auth) 기반 인증 사용

## 운영 메모

- 프로덕션은 `my_nas`가 정적 프론트(`front/svelte-front/build`)를 임베드해 서빙합니다.
- API는 `/NAS/api/*`, SPA는 `/NAS/*`, WebDAV는 `/webdav/*` 경로로 제공됩니다.
- 개인/소규모 팀 환경을 기준으로 설계되었습니다.