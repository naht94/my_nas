#!/usr/bin/env bash
# NAS 개발/운영 스택(my_nas + proxy, --dev 시 vite) 일괄 실행/종료
#
# 사용법:
#   ./scripts/nas.sh start [--build] [--dev]
#   ./scripts/nas.sh stop    # 또는 down
#   ./scripts/nas.sh restart [--build] [--dev]
#   ./scripts/nas.sh status
#   ./scripts/nas.sh logs [my_nas|vite|proxy|all]

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_DIR="${ROOT}/.run"
FRONT_DIR="${ROOT}/front/svelte-front"

MY_NAS_PID="${RUN_DIR}/my_nas.pid"
VITE_PID="${RUN_DIR}/vite.pid"
PROXY_PID="${RUN_DIR}/proxy.pid"

MY_NAS_LOG="${RUN_DIR}/my_nas.log"
VITE_LOG="${RUN_DIR}/vite.log"
PROXY_LOG="${RUN_DIR}/proxy.log"

USE_DEV=0
DO_BUILD=0

usage() {
  sed -n '2,10p' "$0" | sed 's/^# \?//'
}

load_env() {
  if [[ ! -f "${ROOT}/.env" ]]; then
    echo "오류: ${ROOT}/.env 가 없습니다. .env.example 을 참고해 만드세요." >&2
    exit 1
  fi
  set -a
  # shellcheck disable=SC1091
  source "${ROOT}/.env"
  set +a

  : "${DATABASE_URL:?DATABASE_URL 가 .env 에 필요합니다}"
  : "${STORAGE_PATH:?STORAGE_PATH 가 .env 에 필요합니다}"
  : "${CERT_PATH:?CERT_PATH 가 .env 에 필요합니다 (proxy)}"
  : "${KEY_PATH:?KEY_PATH 가 .env 에 필요합니다 (proxy)}"
}

resolve_bin() {
  local name=$1
  if [[ "${USE_DEV}" -eq 1 ]]; then
    echo "${ROOT}/target/debug/${name}"
    return
  fi
  if [[ -x "${ROOT}/target/release/${name}" ]]; then
    echo "${ROOT}/target/release/${name}"
  elif [[ -x "${ROOT}/target/debug/${name}" ]]; then
    echo "${ROOT}/target/debug/${name}"
  else
    echo ""
  fi
}

is_running() {
  local pidfile=$1
  [[ -f "${pidfile}" ]] || return 1
  local pid
  pid="$(<"${pidfile}")"
  [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null
}

# PID와 자식 프로세스를 재귀적으로 종료한다.
kill_tree() {
  local pid=$1
  local sig=${2:-TERM}
  local child
  while read -r child; do
    [[ -n "${child}" && "${child}" != "${pid}" ]] || continue
    kill_tree "${child}" "${sig}"
  done < <(pgrep -P "${pid}" 2>/dev/null || true)
  kill "-${sig}" "${pid}" 2>/dev/null || true
}

stop_one() {
  local name=$1
  local pidfile=$2
  local stopped=0

  if [[ -f "${pidfile}" ]]; then
    local pid
    pid="$(<"${pidfile}")"
    if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
      # setsid 세션 + npm/vite 자식까지 종료
      kill -- -"${pid}" 2>/dev/null || true
      kill_tree "${pid}" TERM
      for _ in $(seq 1 20); do
        kill -0 "${pid}" 2>/dev/null || { stopped=1; break; }
        sleep 0.25
      done
      if kill -0 "${pid}" 2>/dev/null; then
        kill -- -"${pid}" 2>/dev/null || true
        kill_tree "${pid}" KILL
      fi
      stopped=1
    fi
    rm -f "${pidfile}"
  fi

  if [[ "${stopped}" -eq 1 ]]; then
    echo "  stopped ${name}"
  else
    echo "  ${name} was not running (no pidfile or stale pid)"
  fi
}

# pidfile이 없거나 자식이 남았을 때 이름/포트로 정리
cleanup_fallback() {
  local cleaned=0

  if pgrep -x my_nas >/dev/null 2>&1; then
    pkill -x my_nas 2>/dev/null || true
    echo "  fallback: killed my_nas"
    cleaned=1
  fi
  if pgrep -x proxy >/dev/null 2>&1; then
    pkill -x proxy 2>/dev/null || true
    echo "  fallback: killed proxy"
    cleaned=1
  fi
  # vite / npm dev (5173)
  if pgrep -f "vite.*5173" >/dev/null 2>&1 || pgrep -f "vite dev" >/dev/null 2>&1; then
    pkill -f "vite.*5173" 2>/dev/null || true
    pkill -f "vite dev" 2>/dev/null || true
    echo "  fallback: killed vite dev"
    cleaned=1
  fi

  [[ "${cleaned}" -eq 0 ]] || true
}

start_one() {
  local name=$1
  local pidfile=$2
  local logfile=$3
  shift 3

  if is_running "${pidfile}"; then
    echo "  ${name} already running (pid $(<"${pidfile}"))"
    return 0
  fi

  mkdir -p "${RUN_DIR}"
  : > "${logfile}"
  setsid "$@" >>"${logfile}" 2>&1 &
  echo $! >"${pidfile}"
  echo "  started ${name} (pid $(<"${pidfile}"), log ${logfile})"
}

build_all() {
  echo "==> frontend build"
  if [[ ! -d "${FRONT_DIR}/node_modules" ]]; then
    (cd "${FRONT_DIR}" && npm install)
  fi
  if [[ -x "${FRONT_DIR}/node_modules/.bin/vite" ]]; then
    (cd "${FRONT_DIR}" && node node_modules/vite/bin/vite.js build)
  else
    (cd "${FRONT_DIR}" && npm run build)
  fi

  echo "==> backend build (${USE_DEV:+debug}${USE_DEV:-release})"
  if [[ "${USE_DEV}" -eq 1 ]]; then
    (cd "${ROOT}" && cargo build --bin my_nas --bin proxy)
  else
    (cd "${ROOT}" && cargo build --release --bin my_nas --bin proxy)
  fi
}

cmd_start() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --build) DO_BUILD=1 ;;
      --dev) USE_DEV=1 ;;
      *) echo "알 수 없는 옵션: $1" >&2; usage; exit 1 ;;
    esac
    shift
  done

  load_env
  [[ "${DO_BUILD}" -eq 1 ]] && build_all

  local my_nas_bin proxy_bin
  my_nas_bin="$(resolve_bin my_nas)"
  proxy_bin="$(resolve_bin proxy)"

  if [[ -z "${my_nas_bin}" || -z "${proxy_bin}" ]]; then
    echo "오류: my_nas 또는 proxy 바이너리가 없습니다. ./scripts/nas.sh start --build 를 실행하세요." >&2
    exit 1
  fi
  if [[ ! -d "${FRONT_DIR}/node_modules" ]]; then
    echo "==> npm install (frontend)"
    (cd "${FRONT_DIR}" && npm install)
  fi

  echo "==> starting NAS stack"
  start_one my_nas "${MY_NAS_PID}" "${MY_NAS_LOG}" env \
    DATABASE_URL="${DATABASE_URL}" \
    STORAGE_PATH="${STORAGE_PATH}" \
    RUST_LOG="${RUST_LOG:-info}" \
    "${my_nas_bin}"

  local proxy_env=(CERT_PATH="${CERT_PATH}" KEY_PATH="${KEY_PATH}")

  if [[ "${USE_DEV}" -eq 1 ]]; then
    start_one vite "${VITE_PID}" "${VITE_LOG}" \
      bash -lc "cd '${FRONT_DIR}' && exec npm run dev -- --host 0.0.0.0 --port 5173"
    proxy_env+=(NAS_UI_UPSTREAM_PORT=5173)
    echo ""
    echo "Ready (dev: vite HMR + API proxy)."
    echo "  Web UI : https://<host>:48483/NAS  (proxy -> vite:5173, /NAS/api -> my_nas)"
    echo "  API    : https://<host>:48484/NAS  (proxy -> my_nas:3000)"
  else
    if [[ ! -d "${FRONT_DIR}/build" ]]; then
      echo "오류: 프론트 빌드가 없습니다. ./scripts/nas.sh start --build 를 실행하세요." >&2
      exit 1
    fi
    echo ""
    echo "Ready (production: embedded static UI + API on my_nas)."
    echo "  NAS    : https://<host>:48483/NAS  (UI + API, proxy -> my_nas:3000)"
    echo "  (48484 도 동일 — WebDAV·기존 API 북마크 호환)"
  fi

  start_one proxy "${PROXY_PID}" "${PROXY_LOG}" env \
    "${proxy_env[@]}" \
    "${proxy_bin}"

  echo ""
  echo "  logs   : ./scripts/nas.sh logs"
  echo "  stop   : ./scripts/nas.sh stop"
  if [[ "${USE_DEV}" -eq 1 ]]; then
    echo "  dev    : vite HMR — ./scripts/nas.sh restart --dev"
  fi
}

cmd_stop() {
  echo "==> stopping NAS stack"
  stop_one proxy "${PROXY_PID}"
  stop_one vite "${VITE_PID}"
  stop_one my_nas "${MY_NAS_PID}"
  cleanup_fallback
  echo "done. (다시 확인: ./scripts/nas.sh status)"
}

cmd_status() {
  status_one() {
    local name=$1
    local pidfile=$2
    local logfile=$3
    if is_running "${pidfile}"; then
      printf "  %-8s running  pid=%-8s log=%s\n" "${name}" "$(<"${pidfile}")" "${logfile}"
    else
      printf "  %-8s stopped\n" "${name}"
      rm -f "${pidfile}"
    fi
  }

  echo "NAS stack status (.run/)"
  status_one my_nas "${MY_NAS_PID}" "${MY_NAS_LOG}"
  status_one vite "${VITE_PID}" "${VITE_LOG}"
  status_one proxy "${PROXY_PID}" "${PROXY_LOG}"
}

cmd_logs() {
  local target="${1:-all}"
  tail_one() {
    local name=$1
    local file=$2
    if [[ -f "${file}" ]]; then
      echo "=== ${name} (${file}) ==="
      tail -n 30 "${file}"
      echo ""
    fi
  }

  case "${target}" in
    my_nas) tail_one my_nas "${MY_NAS_LOG}" ;;
    vite) tail_one vite "${VITE_LOG}" ;;
    proxy) tail_one proxy "${PROXY_LOG}" ;;
    all)
      tail_one my_nas "${MY_NAS_LOG}"
      tail_one vite "${VITE_LOG}"
      tail_one proxy "${PROXY_LOG}"
      ;;
    *)
      echo "대상: my_nas | vite | proxy | all" >&2
      exit 1
      ;;
  esac
}

cmd_restart() {
  cmd_stop
  sleep 1
  cmd_start "$@"
}

main() {
  local cmd="${1:-}"
  shift || true
  case "${cmd}" in
    start) cmd_start "$@" ;;
    stop|down) cmd_stop ;;
    restart) cmd_restart "$@" ;;
    status) cmd_status ;;
    logs) cmd_logs "${1:-all}" ;;
    -h|--help|help|"")
      usage
      ;;
    *)
      echo "알 수 없는 명령: ${cmd}" >&2
      usage
      exit 1
      ;;
  esac
}

main "$@"
