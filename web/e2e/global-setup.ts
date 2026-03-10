import { execSync } from 'node:child_process'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const PROJECT_ROOT = resolve(__dirname, '../..')
const IMAGE_NAME = 'openposterdb-test'
const CONTAINER_NAME = 'openposterdb-test'
const BACKEND_URL = 'http://127.0.0.1:3333/api/auth/status'

function containerCmd(): string {
  try {
    execSync('podman --version', { stdio: 'ignore' })
    return 'podman'
  } catch {
    return 'docker'
  }
}

export default async function globalSetup() {
  const cmd = containerCmd()

  console.log(`[e2e] Building container image via "${cmd}"...`)
  execSync(
    `${cmd} build -t ${IMAGE_NAME} --build-arg CARGO_FEATURES=test-support -f Containerfile .`,
    { cwd: PROJECT_ROOT, stdio: 'inherit' },
  )

  console.log('[e2e] Starting container...')
  try { execSync(`${cmd} rm -f ${CONTAINER_NAME}`, { stdio: 'ignore' }) } catch { /* ignore */ }
  execSync(
    [
      `${cmd} run -d --name ${CONTAINER_NAME}`,
      '-p 3333:3000',
      '--tmpfs /tmp/openposterdb-e2e',
      '-e TMDB_API_KEY=test',
      '-e MDBLIST_API_KEY=test',
      '-e JWT_SECRET=abababababababababababababababababababababababababababababababab',
      '-e LISTEN_ADDR=0.0.0.0:3000',
      '-e COOKIE_SECURE=false',
      '-e FANART_API_KEY=test',
      '-e CACHE_DIR=/tmp/openposterdb-e2e',
      IMAGE_NAME,
    ].join(' '),
    { cwd: PROJECT_ROOT, stdio: 'inherit' },
  )

  console.log('[e2e] Waiting for backend to be ready...')
  const deadline = Date.now() + 60_000
  while (Date.now() < deadline) {
    try {
      const res = await fetch(BACKEND_URL)
      if (res.ok) {
        console.log('[e2e] Backend is ready')
        return
      }
    } catch {
      // not ready yet
    }
    await new Promise((r) => setTimeout(r, 500))
  }
  throw new Error('Backend did not start within 60 seconds')
}
